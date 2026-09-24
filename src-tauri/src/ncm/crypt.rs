//! ncm 容器解密：AES-128-ECB 解出流密钥 → 构建 RC4 变种 keybox → 分块流式 XOR。
//! 行为与 C++ 版逐块一致（含每个 0x8000 块内 i 从 0 重计的细节）。
//! open() 时预读并解密首个数据块，提前识别 mp3/flac，供转换前计算输出路径与去重。

use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;

use aes::cipher::{BlockDecrypt, KeyInit};
use aes::Aes128;

use super::keys;
use super::metadata::{self, NcmMetadata};

const CHUNK_SIZE: usize = 0x8000;

/// 解密后识别出的音频容器格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Mp3,
    Flac,
}

impl AudioFormat {
    pub fn extension(self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Flac => "flac",
        }
    }
}

#[derive(Debug)]
pub struct NcmError(pub String);

impl std::fmt::Display for NcmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for NcmError {}

fn err<T>(msg: impl Into<String>) -> Result<T, NcmError> {
    Err(NcmError(msg.into()))
}

/// AES-128-ECB 解密并去除最后一个块的填充（与 C++ aesEcbDecrypt 的宽松规则一致：
/// pad = 末字节，>16 视为 0）
fn aes_ecb_decrypt_strip(key: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new(key.into());
    let n_blocks = data.len() >> 4;
    let mut out = Vec::with_capacity(data.len());
    for block_idx in 0..n_blocks {
        let mut block = [0u8; 16];
        block.copy_from_slice(&data[block_idx * 16..block_idx * 16 + 16]);
        cipher.decrypt_block((&mut block).into());
        if block_idx == n_blocks - 1 {
            let pad = block[15];
            let pad = if pad > 16 { 0 } else { pad as usize };
            out.extend_from_slice(&block[..16 - pad]);
        } else {
            out.extend_from_slice(&block);
        }
    }
    out
}

/// 构建 RC4 变种 S-box keybox（与 C++ buildKeyBox 一致）
fn build_key_box(key: &[u8]) -> [u8; 256] {
    let mut box_: [u8; 256] = [0; 256];
    for (i, v) in box_.iter_mut().enumerate() {
        *v = i as u8;
    }
    let mut last_byte: u8 = 0;
    let mut key_offset: usize = 0;
    let key_len = key.len();
    for i in 0..256usize {
        let swap = box_[i];
        let c = swap.wrapping_add(last_byte).wrapping_add(key[key_offset]);
        key_offset += 1;
        if key_offset >= key_len {
            key_offset = 0;
        }
        box_[i] = box_[c as usize];
        box_[c as usize] = swap;
        last_byte = c;
    }
    box_
}

/// 对一个数据块做流式 XOR 解密（块内 i 从 0 计，与 C++ 一致）
fn decrypt_chunk(buffer: &mut [u8], keybox: &[u8; 256]) {
    for i in 0..buffer.len() {
        let j = (i + 1) & 0xff;
        buffer[i] ^= keybox
            [(keybox[j] as usize + keybox[(keybox[j] as usize + j) & 0xff] as usize) & 0xff];
    }
}

fn detect_format(head: &[u8]) -> AudioFormat {
    if head.len() >= 3 && &head[..3] == b"ID3" {
        AudioFormat::Mp3
    } else {
        AudioFormat::Flac
    }
}

/// 已打开并解析完头部的 ncm 文件；首个数据块已预读解密
pub struct NcmFile {
    reader: BufReader<File>,
    keybox: [u8; 256],
    pub metadata: Option<NcmMetadata>,
    pub cover: Option<Vec<u8>>,
    first_chunk: Option<Vec<u8>>,
    format: AudioFormat,
}

impl NcmFile {
    /// 音频格式（open 时即已确定）
    pub fn format(&self) -> AudioFormat {
        self.format
    }

    /// 打开并解析 ncm 头部（魔数、密钥、元数据、封面），预读首个音频块
    pub fn open(path: &Path) -> Result<NcmFile, NcmError> {
        let file = File::open(path).map_err(|e| NcmError(format!("无法打开文件: {e}")))?;
        let mut reader = BufReader::with_capacity(1 << 20, file);

        // 魔数校验
        let mut buf4 = [0u8; 4];
        read_exact(&mut reader, &mut buf4)?;
        if u32::from_le_bytes(buf4) != keys::MAGIC_HEADER_1 {
            return err("不是网易保护的 ncm 文件");
        }
        read_exact(&mut reader, &mut buf4)?;
        if u32::from_le_bytes(buf4) != keys::MAGIC_HEADER_2 {
            return err("不是网易保护的 ncm 文件");
        }

        // 跳过 2 字节 gap
        skip_bytes(&mut reader, 2)?;

        // 音频流密钥
        read_exact(&mut reader, &mut buf4)?;
        let key_len = u32::from_le_bytes(buf4) as usize;
        if key_len == 0 {
            return err("ncm 文件损坏（密钥长度为 0）");
        }
        let mut key_data = vec![0u8; key_len];
        read_exact(&mut reader, &mut key_data)?;
        for b in key_data.iter_mut() {
            *b ^= 0x64;
        }
        let key_decrypted = aes_ecb_decrypt_strip(&keys::CORE_KEY, &key_data);
        if key_decrypted.len() <= keys::KEY_PREFIX_LEN {
            return err("ncm 文件损坏（密钥解密失败）");
        }
        let keybox = build_key_box(&key_decrypted[keys::KEY_PREFIX_LEN..]);

        // 元数据
        read_exact(&mut reader, &mut buf4)?;
        let meta_len = u32::from_le_bytes(buf4) as usize;
        let metadata = if meta_len > 0 {
            let mut meta_data = vec![0u8; meta_len];
            read_exact(&mut reader, &mut meta_data)?;
            for b in meta_data.iter_mut() {
                *b ^= 0x63;
            }
            parse_metadata_block(&meta_data)
        } else {
            None
        };

        // 跳过 crc32(4) + image version(1)
        skip_bytes(&mut reader, 5)?;

        // 封面
        read_exact(&mut reader, &mut buf4)?;
        let cover_frame_len = u32::from_le_bytes(buf4) as usize;
        read_exact(&mut reader, &mut buf4)?;
        let image_len = u32::from_le_bytes(buf4) as usize;
        let cover = if image_len > 0 {
            let mut image = vec![0u8; image_len];
            read_exact(&mut reader, &mut image)?;
            Some(image)
        } else {
            None
        };
        if cover_frame_len > image_len {
            skip_bytes(&mut reader, cover_frame_len - image_len)?;
        }

        // 预读并解密首个音频块（识别格式 + 提前计算输出路径）
        let mut first = vec![0u8; CHUNK_SIZE];
        let n = read_full(&mut reader, &mut first)?;
        if n == 0 {
            return err("ncm 文件损坏（无音频数据）");
        }
        first.truncate(n);
        decrypt_chunk(&mut first, &keybox);
        let format = detect_format(&first);

        Ok(NcmFile {
            reader,
            keybox,
            metadata,
            cover,
            first_chunk: Some(first),
            format,
        })
    }

    /// 流式解密音频数据并写出（从预读块继续）
    pub fn dump_to(&mut self, out_path: &Path) -> Result<AudioFormat, NcmError> {
        let out = std::fs::File::create(out_path)
            .map_err(|e| NcmError(format!("无法创建输出文件: {e}")))?;
        let mut out = std::io::BufWriter::with_capacity(1 << 20, out);

        if let Some(first) = self.first_chunk.take() {
            out.write_all(&first)
                .map_err(|e| NcmError(format!("写入输出失败: {e}")))?;
        }

        let mut buffer = vec![0u8; CHUNK_SIZE];
        loop {
            let n = match self.reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return err(format!("读取文件失败: {e}")),
            };
            decrypt_chunk(&mut buffer[..n], &self.keybox);
            out.write_all(&buffer[..n])
                .map_err(|e| NcmError(format!("写入输出失败: {e}")))?;
        }
        out.flush().map_err(|e| NcmError(format!("写入输出失败: {e}")))?;
        Ok(self.format)
    }
}

/// 解密并解析元数据块：XOR 后跳 22 字节前缀 → base64 → AES → 跳 "music:" → JSON
fn parse_metadata_block(meta_data: &[u8]) -> Option<NcmMetadata> {
    if meta_data.len() <= 22 {
        return None;
    }
    let b64_input = &meta_data[22..];
    // 过滤 base64 无效字节（换行等）
    let cleaned: Vec<u8> = b64_input
        .iter()
        .copied()
        .filter(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
        .collect();
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&cleaned)
        .ok()?;
    let decrypted = aes_ecb_decrypt_strip(&keys::MODIFY_KEY, &decoded);
    if decrypted.len() <= 6 {
        return None;
    }
    let json_bytes = &decrypted[6..];
    let json_str = String::from_utf8_lossy(json_bytes);
    metadata::parse_metadata(&json_str)
}

fn read_exact<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<(), NcmError> {
    r.read_exact(buf)
        .map_err(|e| NcmError(format!("读取文件失败: {e}")))
}

/// 尽量读满 buf（EOF 时返回实际读到的字节数）
fn read_full<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<usize, NcmError> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return err(format!("读取文件失败: {e}")),
        }
    }
    Ok(filled)
}

fn skip_bytes<R: Read>(r: &mut R, mut n: usize) -> Result<(), NcmError> {
    let mut sink = [0u8; 4096];
    while n > 0 {
        let take = sink.len().min(n);
        read_exact(r, &mut sink[..take])?;
        n -= take;
    }
    Ok(())
}

/// 判断文件扩展名是否为 .ncm
pub fn is_ncm_path(path: &Path) -> bool {
    path.extension()
        .map(|e| e.eq_ignore_ascii_case("ncm"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ncm_path() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ncmdump/test/test.ncm")
    }

    #[test]
    fn decrypt_test_ncm() {
        let path = test_ncm_path();
        if !path.exists() {
            eprintln!("skip: test.ncm not found");
            return;
        }
        let mut ncm = NcmFile::open(&path).expect("open ncm");
        assert!(ncm.metadata.is_some(), "test.ncm 应含元数据");
        let meta = ncm.metadata.as_ref().unwrap();
        assert!(!meta.music_name.is_empty(), "歌名不应为空");
        assert!(
            ncm.cover.is_some() || meta.album_pic_url.is_some(),
            "应有内嵌封面或封面 URL"
        );

        let out = std::env::temp_dir().join("ncm-dump-gui-test-output.bin");
        let format = ncm.dump_to(&out).expect("dump");
        assert_eq!(format, ncm.format(), "dump 前后格式一致");

        let mut f = File::open(&out).unwrap();
        let mut head = [0u8; 4];
        f.read_exact(&mut head).unwrap();
        match format {
            AudioFormat::Mp3 => assert_eq!(&head[..3], b"ID3"),
            AudioFormat::Flac => assert_eq!(&head[..4], b"fLaC"),
        }
        let size = out.metadata().unwrap().len();
        assert!(size > 100_000, "输出音频应有实际内容，got {size} bytes");
        let _ = std::fs::remove_file(&out);
    }
}

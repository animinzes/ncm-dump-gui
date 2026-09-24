//! 用 lofty 写全量标签：mp3→ID3v2，flac→Vorbis Comment + PICTURE。
//! 字段：标题/歌手/专辑/专辑歌手(TPE2)/音轨号/碟号/年份/歌词(USLT·LYRICS)/封面(APIC·PICTURE)。

use std::path::Path;

use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::Tag;

#[derive(Debug, Clone, Default)]
pub struct TagData {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: Option<String>,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub year: Option<u32>,
    pub lyrics: Option<String>,
    /// 封面图片字节（PNG/JPEG 原始数据）
    pub cover: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct ApplyError(pub String);

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ApplyError {}

/// 将标签写入音频文件（原地修改）
pub fn apply_tags(path: &Path, data: &TagData) -> Result<(), ApplyError> {
    let tagged_file = Probe::open(path)
        .map_err(|e| ApplyError(format!("打开音频文件失败: {e}")))?
        .read()
        .map_err(|e| ApplyError(format!("解析音频文件失败: {e}")))?;

    // 取主标签（没有则按文件格式新建：mp3→ID3v2，flac→Vorbis）
    let tag_type = tagged_file.primary_tag_type();
    let mut tag = tagged_file
        .primary_tag()
        .cloned()
        .unwrap_or_else(|| Tag::new(tag_type));

    tag.set_title(data.title.clone());
    tag.set_artist(data.artist.clone());
    tag.set_album(data.album.clone());
    if let Some(aa) = &data.album_artist {
        tag.insert_text(ItemKey::AlbumArtist, aa.clone());
    }
    if let Some(track) = data.track {
        tag.set_track(track);
    }
    if let Some(disc) = data.disc {
        tag.set_disk(disc);
    }
    if let Some(year) = data.year {
        tag.set_year(year);
    }
    if let Some(lyrics) = &data.lyrics {
        tag.insert_text(ItemKey::Lyrics, lyrics.clone());
    }
    if let Some(cover) = &data.cover {
        let mime = if cover.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            MimeType::Png
        } else {
            MimeType::Jpeg
        };
        tag.remove_picture_type(PictureType::CoverFront);
        tag.push_picture(Picture::new_unchecked(
            PictureType::CoverFront,
            Some(mime),
            None,
            cover.clone(),
        ));
    }

    tag.save_to_path(path, WriteOptions::default())
        .map_err(|e| ApplyError(format!("保存标签失败: {e}")))?;
    Ok(())
}

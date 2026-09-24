//! ncm 内嵌元数据解析：全字段（含 musicId/albumId/albumPic 等网络补全用的 ID 字段）。

use serde::{Deserialize, Serialize};

/// ncm 内嵌 JSON 解析结果（字段全部可选，兼容缺元数据的文件）
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct NcmMetadata {
    pub music_id: Option<i64>,
    pub music_name: String,
    pub artists: Vec<String>,
    pub album_id: Option<i64>,
    pub album: String,
    /// 元数据内嵌的专辑封面直链 URL（网易云 CDN）
    pub album_pic_url: Option<String>,
    pub bitrate: Option<i64>,
    /// 毫秒
    pub duration_ms: Option<i64>,
    pub format: Option<String>,
}

/// 解析元数据 JSON（宽容模式：字段缺失/类型不符一律回退默认）
pub fn parse_metadata(json: &str) -> Option<NcmMetadata> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    Some(NcmMetadata {
        music_id: v.get("musicId").and_then(|x| x.as_i64()),
        music_name: v
            .get("musicName")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        artists: parse_artists(&v),
        album_id: v.get("albumId").and_then(|x| x.as_i64()),
        album: v
            .get("album")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        album_pic_url: v
            .get("albumPic")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        bitrate: v.get("bitrate").and_then(|x| x.as_i64()),
        duration_ms: v.get("duration").and_then(|x| x.as_i64()),
        format: v
            .get("format")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
    })
}

/// artist 字段是二维数组：[[名字, id], ...]，取每项第一个字符串元素
fn parse_artists(v: &serde_json::Value) -> Vec<String> {
    let arr = match v.get("artist").and_then(|x| x.as_array()) {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut names = Vec::new();
    for entry in arr {
        if let Some(inner) = entry.as_array() {
            if let Some(name) = inner.first().and_then(|x| x.as_str()) {
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

impl NcmMetadata {
    /// artist 连接形式（无歌手时为空串）
    pub fn artist_joined(&self) -> String {
        self.artists.join(" / ")
    }

    pub fn has_meaningful_data(&self) -> bool {
        !self.music_name.is_empty() || !self.album.is_empty() || !self.artists.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_metadata() {
        let json = r#"{
            "musicId": 347230,
            "musicName": "海阔天空",
            "artist": [["Beyond", 11127]],
            "albumId": 34209,
            "album": "海阔天空",
            "albumPic": "https://p1.music.126.net/xxx/yyy.jpg",
            "bitrate": 797831,
            "duration": 326000,
            "format": "flac"
        }"#;
        let m = parse_metadata(json).unwrap();
        assert_eq!(m.music_id, Some(347230));
        assert_eq!(m.music_name, "海阔天空");
        assert_eq!(m.artists, vec!["Beyond"]);
        assert_eq!(m.artist_joined(), "Beyond");
        assert_eq!(m.album_id, Some(34209));
        assert_eq!(m.album_pic_url.as_deref(), Some("https://p1.music.126.net/xxx/yyy.jpg"));
        assert_eq!(m.format.as_deref(), Some("flac"));
        assert!(m.has_meaningful_data());
    }

    #[test]
    fn parse_multi_artists() {
        let json = r#"{
            "musicName": "合唱曲",
            "artist": [["A", 1], ["B", 2], ["C", 3]],
            "album": "合集"
        }"#;
        let m = parse_metadata(json).unwrap();
        assert_eq!(m.artist_joined(), "A / B / C");
    }

    #[test]
    fn parse_sparse_metadata() {
        let m = parse_metadata(r#"{"musicName":"只有歌名"}"#).unwrap();
        assert_eq!(m.music_name, "只有歌名");
        assert_eq!(m.artists.len(), 0);
        assert!(m.music_id.is_none());
        assert!(m.album_pic_url.is_none());
    }

    #[test]
    fn parse_garbage_returns_none() {
        assert!(parse_metadata("not json").is_none());
    }
}

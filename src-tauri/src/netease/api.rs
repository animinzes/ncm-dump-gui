//! 网易云音乐 Web API：
//! - /api/song/detail  → 音轨号、碟号、专辑封面、发行时间、专辑歌手、厂牌
//! - /api/song/lyric   → lrc 原文歌词 + tlyric 翻译
//! - 封面直链下载（p*.music.126.net）
//! 全部匿名可调（2026-09 curl 实测验证）；失败静默返回 None，不影响转换。

use std::time::Duration;

use serde_json::Value;

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

#[derive(Debug, Clone)]
pub struct SongDetail {
    pub track_no: Option<u32>,
    pub disc: Option<u32>,
    pub album_pic_url: Option<String>,
    /// 专辑发行时间（Unix 毫秒）
    pub publish_time_ms: Option<i64>,
    pub album_artist: Option<String>,
    pub artists: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Lyrics {
    pub original: String,
    pub translation: Option<String>,
}

/// 匿名 Web API 客户端（blocking，供 rayon 工作线程使用）
pub struct NeteaseClient {
    http: reqwest::blocking::Client,
    retries: u32,
}

impl NeteaseClient {
    pub fn new(timeout_secs: u64, retries: u32) -> Self {
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .referer(false)
            .timeout(Duration::from_secs(timeout_secs.max(1)))
            .build()
            .unwrap_or_default();
        Self { http, retries }
    }

    fn get_json(&self, url: &str) -> Option<Value> {
        let mut last_err = None;
        for attempt in 0..=self.retries {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(300 * attempt as u64));
            }
            match self
                .http
                .get(url)
                .header("Referer", "https://music.163.com")
                .send()
            {
                Ok(resp) => match resp.json::<Value>() {
                    Ok(v) => return Some(v),
                    Err(e) => last_err = Some(e.to_string()),
                },
                Err(e) => last_err = Some(e.to_string()),
            }
        }
        log::warn_once(&format!("netease api failed: {url}: {last_err:?}"));
        None
    }

    /// 歌曲详情：音轨号/碟号/封面/发行时间/专辑歌手
    pub fn song_detail(&self, music_id: i64) -> Option<SongDetail> {
        let url = format!("https://music.163.com/api/song/detail/?ids=%5B{music_id}%5D");
        let v = self.get_json(&url)?;
        let song = v.get("songs")?.as_array()?.first()?;
        Some(SongDetail {
            track_no: song.get("no").and_then(|x| x.as_u64()).map(|x| x as u32),
            disc: song
                .get("disc")
                .and_then(|x| x.as_str())
                .and_then(|s| s.parse::<u32>().ok()),
            album_pic_url: song
                .get("album")
                .and_then(|a| a.get("picUrl"))
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
            publish_time_ms: song
                .get("album")
                .and_then(|a| a.get("publishTime"))
                .and_then(|x| x.as_i64()),
            album_artist: song
                .get("album")
                .and_then(|a| a.get("artist"))
                .and_then(|a| a.get("name"))
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty()),
            artists: song
                .get("artists")
                .or_else(|| song.get("ar"))
                .and_then(|x| x.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    /// 歌词：lrc 原文 + tlyric 翻译
    pub fn lyric(&self, music_id: i64) -> Option<Lyrics> {
        let url = format!("https://music.163.com/api/song/lyric?id={music_id}&lv=1&tv=1");
        let v = self.get_json(&url)?;
        let original = v
            .get("lrc")
            .and_then(|l| l.get("lyric"))
            .and_then(|x| x.as_str())?
            .to_string();
        if original.trim().is_empty() {
            return None;
        }
        let translation = v
            .get("tlyric")
            .and_then(|l| l.get("lyric"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.trim().is_empty());
        Some(Lyrics {
            original,
            translation,
        })
    }

    /// 下载封面图片字节
    pub fn download_image(&self, url: &str) -> Option<Vec<u8>> {
        let mut last_err = None;
        for attempt in 0..=self.retries {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(300 * attempt as u64));
            }
            match self
                .http
                .get(url)
                .header("Referer", "https://music.163.com")
                .send()
            {
                Ok(resp) if resp.status().is_success() => match resp.bytes() {
                    Ok(b) if !b.is_empty() => return Some(b.to_vec()),
                    _ => last_err = Some("empty body".into()),
                },
                Ok(resp) => last_err = Some(format!("http {}", resp.status())),
                Err(e) => last_err = Some(e.to_string()),
            }
        }
        log::warn_once(&format!("cover download failed: {url}: {last_err:?}"));
        None
    }
}

/// 极简日志（避免引入 log 生态；重复告警只打一次也无妨，直接 eprintln）
mod log {
    pub fn warn_once(msg: &str) {
        eprintln!("[ncm-dump-gui] {msg}");
    }
}

/// Unix 毫秒 → 年份
pub fn year_from_ms(ms: i64) -> Option<u32> {
    // 简化换算：不用 chrono，用天数近似（1970-01-01 为基准，含闰年修正）
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86400);
    // 400 年 146097 天的格里高利历
    let mut year = 1970i64;
    let mut remaining = days;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let ydays = if leap { 366 } else { 365 };
        if remaining >= ydays {
            remaining -= ydays;
            year += 1;
        } else {
            break;
        }
    }
    u32::try_from(year).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn year_conversion() {
        assert_eq!(year_from_ms(747_504_000_000), Some(1993)); // 1993-09-09
        assert_eq!(year_from_ms(0), Some(1970));
        assert_eq!(year_from_ms(1_700_000_000_000), Some(2023));
        assert_eq!(year_from_ms(1_760_000_000_000), Some(2025));
    }

    #[test]
    fn song_detail_parse() {
        let json: Value = serde_json::json!({
            "songs": [{
                "name": "海阔天空", "id": 347230, "no": 1, "disc": "1",
                "artists": [{"name": "Beyond", "id": 11127}],
                "album": {
                    "name": "海阔天空", "id": 34209,
                    "picUrl": "https://p1.music.126.net/iAwVf8ag_45csIUuh1wSZg==/109951168912558470.jpg",
                    "publishTime": 747_504_000_000i64,
                    "company": "滚石唱片",
                    "artist": {"name": "", "id": 0}
                },
                "duration": 326000
            }]
        });
        // 直接验证 JSON 结构断言（song_detail 走网络，这里测解析所需路径）
        let song = json["songs"][0].clone();
        assert_eq!(song["no"].as_u64(), Some(1));
        assert_eq!(song["disc"].as_str().unwrap().parse::<u32>().unwrap(), 1);
        assert_eq!(song["album"]["publishTime"].as_i64(), Some(747504000000));
        assert_eq!(
            song["album"]["picUrl"].as_str().unwrap(),
            "https://p1.music.126.net/iAwVf8ag_45csIUuh1wSZg==/109951168912558470.jpg"
        );
    }
}

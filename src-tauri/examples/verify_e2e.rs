//! 真实端到端：默认配置（网络全开）转换 test.ncm，回读全部写入的标签
use ncm_dump_gui_lib::config::AppConfig;
use ncm_dump_gui_lib::library::{self, NameContext};
use ncm_dump_gui_lib::metadata::tags::{self, TagData};
use ncm_dump_gui_lib::ncm::NcmFile;
use ncm_dump_gui_lib::netease::{year_from_ms, NeteaseClient};

fn main() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ncmdump/test/test.ncm");
    let out_dir = std::env::temp_dir().join("ncm-verify-e2e");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut cfg = AppConfig::default();
    assert_eq!(cfg.output.mode, ncm_dump_gui_lib::config::OutputMode::Library);
    // 验证脚本永远不写真实音乐库：临时切到 custom 模式输出到 temp 目录
    // （归档结构沿用默认 {artist}/{album}，验证目录层级逻辑）
    cfg.output.mode = ncm_dump_gui_lib::config::OutputMode::Custom;
    cfg.output.custom_dir = out_dir.to_string_lossy().into_owned();

    let client = NeteaseClient::new(cfg.network.timeout_secs, cfg.network.retries);
    let mut ncm = NcmFile::open(&src).expect("open");
    let meta = ncm.metadata.clone().expect("metadata");

    // 网络补全
    let mut track = None; let mut disc = None; let mut year = None; let mut aa = None; let mut lyrics = None;
    if let Some(id) = meta.music_id {
        if let Some(d) = client.song_detail(id) {
            track = d.track_no; disc = d.disc;
            year = d.publish_time_ms.and_then(year_from_ms);
            aa = d.album_artist;
        }
        if let Some(l) = client.lyric(id) {
            lyrics = Some(match l.translation { Some(t) => format!("{}\n\n{}", l.original.trim_end(), t), None => l.original });
        }
    }
    let mut cover = ncm.cover.clone();
    if cover.is_none() {
        if let Some(u) = meta.album_pic_url.clone() { cover = client.download_image(&u); }
    }
    println!("补全: track={track:?} disc={disc:?} year={year:?} lyrics={:?} cover={:?}",
        lyrics.as_ref().map(|l| l.len()), cover.as_ref().map(|c| c.len()));

    // 命名与路径（Library 模式 + 默认模板）
    let ctx = NameContext {
        artist: meta.artist_joined(),
        title: meta.music_name.clone(),
        album: meta.album.clone(),
        track, disc, year, bitrate: meta.bitrate,
    };
    let dir = library::resolve_target_dir(cfg.output.mode, &cfg.output.custom_dir, &cfg.output.library_dir, &cfg.output.archive_structure, &src, &ctx);
    std::fs::create_dir_all(&dir).unwrap();
    let stem = library::render_template(&cfg.output.naming_template, &ctx);
    let (target, action) = library::resolve_target_file(&dir, &stem, ncm.format().extension(), cfg.output.dedupe);
    let target = match action { library::TargetAction::Write(p) => p, _ => panic!("unexpected skip") };

    // 解密 + 标签
    ncm.dump_to(&target).expect("dump");
    tags::apply_tags(&target, &TagData {
        title: meta.music_name.clone(), artist: meta.artist_joined(), album: meta.album.clone(),
        album_artist: aa.or_else(|| meta.artists.first().cloned()),
        track, disc, year, lyrics, cover,
    }).expect("tags");

    println!("输出: {}", target.display());

    // 回读断言
    use lofty::file::TaggedFileExt;
    use lofty::prelude::Accessor;
    let tagged = lofty::probe::Probe::open(&target).unwrap().read().unwrap();
    let tag = tagged.primary_tag().expect("tag");
    println!("回读: title={:?} artist={:?} album={:?}", tag.title(), tag.artist(), tag.album());
    println!("      track={:?} disc={:?} year={:?}", tag.track(), tag.disk(), tag.year());
    println!("      pictures={} ({} bytes)", tag.pictures().len(), tag.pictures().first().map(|p| p.data().len()).unwrap_or(0));
    let lyr = tag.get_string(&lofty::tag::ItemKey::Lyrics).map(|s| s.len());
    println!("      lyrics_len={:?}", lyr);

    assert_eq!(tag.title().as_deref(), Some("贝贝"));
    assert_eq!(tag.artist().as_deref(), Some("李荣浩"));
    assert_eq!(tag.album().as_deref(), Some("耳朵"));
    assert_eq!(tag.track(), Some(10));
    assert_eq!(tag.disk(), Some(1));
    assert_eq!(tag.year(), Some(2018));
    assert!(lyr.map(|l| l > 10).unwrap_or(false), "歌词应已嵌入");
    assert!(!tag.pictures().is_empty());
    println!("\n✓ 端到端全部通过");
    let _ = std::fs::remove_dir_all(&out_dir);
}

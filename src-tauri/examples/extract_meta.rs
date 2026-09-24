fn main() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ncmdump/test/test.ncm");
    let ncm = ncm_dump_gui_lib::ncm::NcmFile::open(&p).expect("open");
    if let Some(m) = ncm.metadata {
        println!("musicId: {:?}", m.music_id);
        println!("name: {}", m.music_name);
        println!("artist: {}", m.artist_joined());
        println!("album: {} (id {:?})", m.album, m.album_id);
        println!("albumPic: {:?}", m.album_pic_url);
        println!("format: {:?} bitrate: {:?}", m.format, m.bitrate);
    } else {
        println!("no metadata");
    }
}

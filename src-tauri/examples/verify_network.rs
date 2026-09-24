//! 用 test.ncm 的真实 musicId 验证网络补全管线
use ncm_dump_gui_lib::netease::NeteaseClient;

fn main() {
    let client = NeteaseClient::new(10, 2);
    let music_id = 1318234987i64; // 李荣浩 - 贝贝

    println!("=== song_detail ===");
    match client.song_detail(music_id) {
        Some(d) => {
            println!("track_no: {:?}", d.track_no);
            println!("disc: {:?}", d.disc);
            println!("album_artist: {:?}", d.album_artist);
            println!("publish_time_ms: {:?} (year {:?})", d.publish_time_ms, d.publish_time_ms.map(ncm_dump_gui_lib::netease::year_from_ms));
            println!("artists: {:?}", d.artists);
            println!("album_pic: {:?}", d.album_pic_url.as_deref().map(|u| &u[..60.min(u.len())]));

            println!("\n=== lyric ===");
            match client.lyric(music_id) {
                Some(l) => {
                    let orig: Vec<&str> = l.original.lines().take(3).collect();
                    println!("lrc head: {:?}", orig);
                    println!("translation: {:?}", l.translation.as_deref().map(|t| &t[..t.len().min(50)]));
                }
                None => println!("no lyric"),
            }

            println!("\n=== cover download (albumPic from ncm metadata) ===");
            let url = "http://p4.music.126.net/tt8xwK-ASC2iqXNUXYKoDQ==/109951163606377163.jpg";
            match client.download_image(url) {
                Some(bytes) => {
                    let is_jpg = bytes.len() > 2 && bytes[0] == 0xFF && bytes[1] == 0xD8;
                    println!("downloaded {} bytes, jpeg={}", bytes.len(), is_jpg);
                }
                None => println!("download failed"),
            }
            if let Some(pic) = &d.album_pic_url {
                match client.download_image(pic) {
                    Some(b) => println!("api cover: {} bytes", b.len()),
                    None => println!("api cover download failed"),
                }
            }
        }
        None => println!("song_detail failed"),
    }
}

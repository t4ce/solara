//! Inspect locally fetched HTML without displaying pages or downloading video.
use solara::watch_media;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: watch_resolve WATCH_URL WATCH_HTML PLAYER_HTML".into());
    }
    let page = url::Url::parse(&args[1])?;
    if !watch_media::is_watch(&page) {
        return Err("Unsupported watch URL".into());
    }
    let player = watch_media::player_frame(&std::fs::read_to_string(&args[2])?, &page)?;
    let media = watch_media::media_source(&std::fs::read_to_string(&args[3])?, &player)?;
    // Signed query tokens are deliberately omitted from probe output.
    println!(
        "resolved HTTPS MP4: host={} path={}",
        media.host_str().unwrap_or(""),
        media.path()
    );
    Ok(())
}

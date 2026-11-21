fn main() -> anyhow::Result<()> {
    ffmpeg_next::init()?;
    println!("{}", ffmpeg_next::codec::version());
    Ok(())
}

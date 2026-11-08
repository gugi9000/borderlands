use std::{
    env,
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

use borderlands::map_generator;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h") {
        println!("Usage: generate-map SIZE NAME");
        println!("SIZE: odd number from 11 to 101. Quote names containing spaces.");
        println!("Writes maps/<lower-case-name>-<size>.map without overwriting existing files.");
        return Ok(());
    }
    if args.len() != 2 {
        return Err("expected SIZE and NAME; use generate-map --help".into());
    }
    let size = args[0].parse::<usize>()?;
    let map = map_generator::generate(size, &args[1])?;
    let directory = PathBuf::from("maps");
    fs::create_dir_all(&directory)?;
    let path = directory.join(map.file_name());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| format!("could not create {}: {error}", path.display()))?;
    file.write_all(map.text.as_bytes())?;
    file.sync_all()?;
    println!("Saved {} (seed {:016x})", path.display(), map.seed);
    Ok(())
}

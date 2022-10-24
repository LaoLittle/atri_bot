use std::env::current_dir;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub mod config;

pub static DATA_PATH: &str = "data";

pub struct Holder<T, S, D> {
    path: PathBuf,
    data: T,
    ser: S,
    deser: D,
}

impl<T, S, D> Holder<T, S, D>
where
    S: Fn(Option<&[u8]>) -> T,
    D: Fn(&T) -> &[u8],
{
    pub fn new<P>(path: P, ser: S, deser: D) -> Self
    where
        P: AsRef<Path>,
    {
        let p = path.as_ref();

        let result = std::fs::read(p);

        let data = if let Ok(bytes) = result {
            ser(Some(&bytes))
        } else {
            ser(None)
        };

        Self {
            path: p.to_path_buf(),
            data,
            ser,
            deser,
        }
    }

    pub fn reload<F: FnOnce(&[u8]) -> T>(&mut self, data: F) -> std::io::Result<()> {
        let file = File::open(&self.path)?;
        let mut bytes = vec![];

        (&file).read_to_end(&mut bytes)?;

        let data = data(&bytes);
        self.data = data;

        Ok(())
    }

    pub fn store<F: FnOnce(&T) -> &[u8]>(&self, store: F) -> std::io::Result<()> {
        let bytes = store(&self.data);

        let f = File::create(&self.path)?;
        (&f).write_all(bytes)?;

        Ok(())
    }
}

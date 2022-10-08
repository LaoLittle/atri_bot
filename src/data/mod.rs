use std::env::current_dir;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub mod config;

pub static DATA_PATH: &str = "data";

struct Holder<T> {
    path: PathBuf,
    data: T,
}

impl<T> Holder<T> {
    pub fn new<P, F>(path: P, data: F) -> std::io::Result<Self>
    where
        P: AsRef<Path>,
        F: FnOnce(Option<&[u8]>) -> T,
    {
        let p = path.as_ref();

        let path = current_dir()
            .map(|buf| buf.join(p))
            .unwrap_or_else(|_| p.to_path_buf());

        let result = File::open(&path).and_then(|f| {
            let mut bytes = vec![];
            (&f).read_to_end(&mut bytes)?;

            Ok(bytes)
        });

        let data = if let Ok(bytes) = result {
            data(Some(&bytes))
        } else {
            data(None)
        };

        Ok(Self { path, data })
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

pub struct Data<T>(Holder<T>);

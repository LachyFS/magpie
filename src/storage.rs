use crate::model::{ImageItem, Library};
use anyhow::{Context, Result};
use image::{GenericImageView, ImageReader};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct Storage {
    pub root: PathBuf,
    _lock: Arc<fs::File>,
}

impl Storage {
    pub fn open(root: Option<PathBuf>) -> Result<Self> {
        let root = root
            .or_else(|| std::env::var_os("MAGPIE_DATA_DIR").map(PathBuf::from))
            .or_else(|| {
                directories::ProjectDirs::from("app", "Magpie", "Magpie")
                    .map(|p| p.data_dir().to_owned())
            })
            .context("Couldn't find a folder for your boards")?;
        fs::create_dir_all(root.join("images"))?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("library.lock"))?;
        lock.try_lock()
            .context("This library is already open in another Magpie window")?;
        Ok(Self {
            root,
            _lock: Arc::new(lock),
        })
    }

    pub fn load(&self) -> Result<Library> {
        let path = self.root.join("boards.json");
        if !path.exists() {
            return Ok(Library::default());
        }
        let library: Library = serde_json::from_slice(&fs::read(path)?)
            .context("Couldn't read boards.json; it has been left untouched")?;
        library.validate()?;
        Ok(library)
    }

    pub fn save(&self, library: &Library) -> Result<()> {
        library.validate()?;
        let bytes = serde_json::to_vec_pretty(library)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist(self.root.join("boards.json"))?;
        Ok(())
    }

    pub fn asset(&self, name: &str) -> PathBuf {
        self.root.join("images").join(name)
    }

    pub fn import_path(&self, path: &Path) -> Result<ImageItem> {
        anyhow::ensure!(
            fs::metadata(path)?.len() <= 100 * 1024 * 1024,
            "Images must be smaller than 100 MB"
        );
        let bytes = fs::read(path)?;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.import_bytes(&bytes, &name)
    }

    pub fn import_bytes(&self, bytes: &[u8], name: &str) -> Result<ImageItem> {
        anyhow::ensure!(
            bytes.len() <= 100 * 1024 * 1024,
            "Images must be smaller than 100 MB"
        );
        let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
        let format = reader
            .format()
            .context("Unsupported image. Try PNG, JPEG, WebP, GIF, BMP, or TIFF.")?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(24000);
        limits.max_image_height = Some(24000);
        limits.max_alloc = Some(256 * 1024 * 1024);
        reader.limits(limits);
        let mut image = reader.decode().context("Couldn't decode this image")?;
        // Respect phone/camera orientation before calculating the canvas aspect ratio.
        if let Ok(mut decoder) = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()?
            .into_decoder()
        {
            use image::ImageDecoder;
            if let Ok(orientation) = decoder.orientation() {
                image.apply_orientation(orientation);
            }
        }
        let (width, height) = image.dimensions();
        anyhow::ensure!(width > 0 && height > 0, "Empty image");
        let id = Uuid::new_v4();
        let original = format!("{id}.{}", format.extensions_str()[0]);
        let asset = format!("{id}-preview.png");
        fs::write(self.asset(&original), bytes)?;
        image
            .thumbnail(2048, 2048)
            .save_with_format(self.asset(&asset), image::ImageFormat::Png)?;
        let scale = (360.0 / width as f64).min(420.0 / height as f64).min(1.0);
        Ok(ImageItem {
            id,
            asset,
            original,
            name: name.into(),
            x: 0.0,
            y: 0.0,
            width: width as f64 * scale,
            height: height as f64 * scale,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_instances_cannot_overwrite_the_same_library() {
        let dir = tempfile::tempdir().unwrap();
        let store = Storage::open(Some(dir.path().into())).unwrap();
        let background_worker = store.clone();
        assert!(Storage::open(Some(dir.path().into())).is_err());
        drop(store);
        assert!(Storage::open(Some(dir.path().into())).is_err());
        drop(background_worker);
        assert!(Storage::open(Some(dir.path().into())).is_ok());
    }
    #[test]
    fn imports_are_owned_and_boards_roundtrip_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let store = Storage::open(Some(dir.path().join("library"))).unwrap();
        let source = dir.path().join("source.png");
        image::RgbaImage::new(800, 400).save(&source).unwrap();
        let item = store.import_path(&source).unwrap();
        fs::remove_file(source).unwrap();
        assert!(store.asset(&item.asset).exists());
        assert!(store.asset(&item.original).exists());
        assert_eq!((item.width, item.height), (360.0, 180.0));
        let mut library = Library::default();
        library.board_mut().images.push(item);
        store.save(&library).unwrap();
        assert_eq!(store.load().unwrap(), library);
        library.add_board();
        store.save(&library).unwrap();
        assert_eq!(store.load().unwrap(), library);
    }
    #[test]
    fn corrupt_library_is_not_silently_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let store = Storage::open(Some(dir.path().into())).unwrap();
        fs::write(dir.path().join("boards.json"), b"broken").unwrap();
        assert!(store.load().is_err());
        assert_eq!(fs::read(dir.path().join("boards.json")).unwrap(), b"broken");
    }
    #[test]
    fn invalid_images_and_escaping_paths_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = Storage::open(Some(dir.path().into())).unwrap();
        assert!(store.import_bytes(b"not an image", "no.txt").is_err());
        let mut library = Library::default();
        let mut bytes = Cursor::new(Vec::new());
        image::RgbaImage::new(2, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let mut item = store.import_bytes(bytes.get_ref(), "test.png").unwrap();
        item.asset = "../outside.png".into();
        library.board_mut().images.push(item);
        assert!(store.save(&library).is_err());
    }
}

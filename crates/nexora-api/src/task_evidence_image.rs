use base64::Engine;
use image::ImageDecoder;
use nexora_platform::PlatformError;
pub(crate) struct TaskEvidenceImage;
impl TaskEvidenceImage {
    pub fn normalize(value: &str) -> Result<Vec<u8>, PlatformError> {
        if value.len() > 5592408 {
            return Err(PlatformError::Validation("evidence exceeds 4 MB"));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(value)
            .map_err(|_| PlatformError::Validation("invalid evidence encoding"))?;
        if bytes.len() > 4194304 {
            return Err(PlatformError::Validation("evidence exceeds 4 MB"));
        }
        let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|_| PlatformError::Validation("invalid evidence image"))?;
        if !matches!(
            reader.format(),
            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
        ) {
            return Err(PlatformError::Validation(
                "evidence must be PNG, JPEG or WebP",
            ));
        }
        let format = reader
            .format()
            .ok_or(PlatformError::Validation("invalid evidence image"))?;
        let (width, height) = reader
            .into_dimensions()
            .map_err(|_| PlatformError::Validation("invalid evidence image"))?;
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(PlatformError::Validation(
                "evidence dimensions must be between 1 and 4096 pixels",
            ));
        }
        let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(67108864);
        reader.limits(limits);
        let mut decoder = reader
            .into_decoder()
            .map_err(|_| PlatformError::Validation("invalid evidence image"))?;
        let orientation = decoder
            .orientation()
            .map_err(|_| PlatformError::Validation("invalid evidence image"))?;
        let mut image = image::DynamicImage::from_decoder(decoder)
            .map_err(|_| PlatformError::Validation("invalid evidence image"))?;
        image.apply_orientation(orientation);
        let mut output = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut output, image::ImageFormat::Png)
            .map_err(|_| PlatformError::Storage)?;
        if output.get_ref().len() > 8388608 {
            return Err(PlatformError::Validation(
                "normalized evidence exceeds 8 MB",
            ));
        }
        Ok(output.into_inner())
    }
}

use base64::Engine;
use image::ImageDecoder;
use nexora_platform::PlatformError;
pub(crate) struct OrganizationLogo;
impl OrganizationLogo {
    pub fn normalize(value: &str) -> Result<Vec<u8>, PlatformError> {
        if value.len() > 2800000 {
            return Err(PlatformError::Validation("logo exceeds 2 MB"));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(value)
            .map_err(|_| PlatformError::Validation("invalid logo encoding"))?;
        if bytes.len() > 2097152 {
            return Err(PlatformError::Validation("logo exceeds 2 MB"));
        }
        let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|_| PlatformError::Validation("invalid logo image"))?;
        if !matches!(
            reader.format(),
            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
        ) {
            return Err(PlatformError::Validation("logo must be PNG, JPEG or WebP"));
        }
        let format = reader
            .format()
            .ok_or(PlatformError::Validation("invalid logo image"))?;
        let (width, height) = reader
            .into_dimensions()
            .map_err(|_| PlatformError::Validation("invalid logo image"))?;
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(PlatformError::Validation(
                "logo dimensions must be between 1 and 4096 pixels",
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
            .map_err(|_| PlatformError::Validation("invalid logo image"))?;
        let orientation = decoder
            .orientation()
            .map_err(|_| PlatformError::Validation("invalid logo image"))?;
        let mut image = image::DynamicImage::from_decoder(decoder)
            .map_err(|_| PlatformError::Validation("invalid logo image"))?;
        image.apply_orientation(orientation);
        let resized = image.resize(512, 512, image::imageops::FilterType::Lanczos3);
        let mut canvas = image::DynamicImage::new_rgba8(512, 512);
        image::imageops::overlay(
            &mut canvas,
            &resized,
            i64::from((512 - resized.width()) / 2),
            i64::from((512 - resized.height()) / 2),
        );
        let mut output = std::io::Cursor::new(Vec::new());
        canvas
            .write_to(&mut output, image::ImageFormat::Png)
            .map_err(|_| PlatformError::Storage)?;
        Ok(output.into_inner())
    }
}

use crate::organization_logo::OrganizationLogo;
use base64::Engine;
use image::GenericImageView;
#[test]
fn logo_preserves_small_and_rectangular_raster_images_in_a_transparent_square() {
    for (width, height) in [(64, 64), (320, 80), (80, 320), (1, 1)] {
        let source = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            width,
            height,
            image::Rgba([255, 0, 0, 255]),
        ));
        let mut bytes = std::io::Cursor::new(Vec::new());
        source
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let result = OrganizationLogo::normalize(
            &base64::engine::general_purpose::STANDARD.encode(bytes.into_inner()),
        )
        .unwrap();
        let normalized = image::load_from_memory(&result).unwrap();
        assert_eq!((normalized.width(), normalized.height()), (512, 512));
        assert_eq!(normalized.get_pixel(256, 256).0, [255, 0, 0, 255]);
        if width != height {
            assert_eq!(normalized.get_pixel(0, 0).0[3], 0);
        }
    }
}
#[test]
fn invalid_logo_content_is_not_treated_as_a_dimension_error() {
    assert!(matches!(
        OrganizationLogo::normalize(&base64::engine::general_purpose::STANDARD.encode(b"<svg/>")),
        Err(nexora_platform::PlatformError::Validation(
            "logo must be PNG, JPEG or WebP"
        ))
    ));
}

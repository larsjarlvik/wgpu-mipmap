use wgpu::{
    CommandEncoder, Device, ImageCopyTexture, Origin3d, Texture, TextureDescriptor, TextureUsages,
};

use crate::{backends::RenderMipmapGenerator, core::*, util::get_mip_extent};

/// Generates mipmaps for textures with sampled usage.
pub struct CopyMipmapGenerator<'a> {
    generator: &'a RenderMipmapGenerator,
}

impl<'a> CopyMipmapGenerator<'a> {
    // Creates a new `CopyMipmapGenerator` from an existing `RenderMipmapGenerator`
    /// Once created, it can be used repeatedly to generate mipmaps for any
    /// texture supported by the render generator.
    pub fn new(generator: &'a RenderMipmapGenerator) -> Self {
        Self { generator }
    }

    /// Returns the texture usage `CopyMipmapGenerator` requires for mipmap
    /// generation.
    pub fn required_usage() -> TextureUsages {
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST
    }
}

impl<'a> MipmapGenerator for CopyMipmapGenerator<'a> {
    fn generate(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        texture: &Texture,
        texture_descriptor: &TextureDescriptor,
    ) -> Result<(), Error> {
        // Create a temporary texture with half the resolution
        // of the original texture, and one less mip level
        // We'll generate mipmaps into this texture, then
        // copy the results back into the mip levels of the original texture
        let tmp_descriptor = TextureDescriptor {
            label: None,
            size: get_mip_extent(&texture_descriptor.size, 1),
            mip_level_count: texture_descriptor.mip_level_count - 1,
            sample_count: texture_descriptor.sample_count,
            dimension: texture_descriptor.dimension,
            format: texture_descriptor.format,
            usage: RenderMipmapGenerator::required_usage() | TextureUsages::COPY_SRC,
        };
        let tmp_texture = device.create_texture(&tmp_descriptor);
        self.generator.generate_src_dst(
            device,
            encoder,
            &texture,
            &tmp_texture,
            texture_descriptor,
            &tmp_descriptor,
            1,
        )?;
        let mip_count = tmp_descriptor.mip_level_count;
        for i in 0..mip_count {
            encoder.copy_texture_to_texture(
                ImageCopyTexture {
                    texture: &tmp_texture,
                    mip_level: i,
                    origin: Origin3d::default(),
                    aspect: wgpu::TextureAspect::All,
                },
                ImageCopyTexture {
                    texture: &texture,
                    mip_level: i + 1,
                    origin: Origin3d::default(),
                    aspect: wgpu::TextureAspect::All,
                },
                get_mip_extent(&tmp_descriptor.size, i),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::*;

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[allow(dead_code)]
    async fn generate_and_copy_to_cpu_render_slow(
        buffer: &[u8],
        texture_descriptor: &wgpu::TextureDescriptor<'_>,
    ) -> Result<Vec<MipBuffer>, Error> {
        let (_instance, _adaptor, device, queue) = wgpu_setup().await;

        let generator = crate::backends::RenderMipmapGenerator::new_with_format_hints(
            &device,
            &[texture_descriptor.format],
        );
        let fallback = CopyMipmapGenerator::new(&generator);
        Ok(
            generate_and_copy_to_cpu(&device, &queue, &fallback, buffer, texture_descriptor)
                .await?,
        )
    }

    async fn generate_test(texture_descriptor: &wgpu::TextureDescriptor<'_>) -> Result<(), Error> {
        let (_instance, _adapter, device, _queue) = wgpu_setup().await;
        let render = crate::backends::RenderMipmapGenerator::new_with_format_hints(
            &device,
            &[texture_descriptor.format],
        );
        let generator = CopyMipmapGenerator::new(&render);
        let texture = device.create_texture(&texture_descriptor);
        let mut encoder = device.create_command_encoder(&Default::default());
        generator.generate(&device, &mut encoder, &texture, &texture_descriptor)
    }

    #[test]
    fn sanity_check() {
        init();
        // Generate texture data on the CPU
        let size = 511;
        let mip_level_count = 1 + (size as f32).log2() as u32;
        // Create a texture
        let format = wgpu::TextureFormat::R8Unorm;
        let texture_extent = wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        };
        let texture_descriptor = wgpu::TextureDescriptor {
            size: texture_extent,
            mip_level_count,
            format,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            usage: CopyMipmapGenerator::required_usage(),
            label: None,
        };
        futures::executor::block_on(async {
            let res = generate_test(&texture_descriptor).await;
            assert!(res.is_ok());
        });
    }

    #[test]
    fn unsupported_format() {
        init();
        // Generate texture data on the CPU
        let size = 511;
        let mip_level_count = 1 + (size as f32).log2() as u32;
        // Create a texture
        let format = wgpu::TextureFormat::R8Unorm;
        let texture_extent = wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        };
        let texture_descriptor = wgpu::TextureDescriptor {
            size: texture_extent,
            mip_level_count,
            format,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            usage: wgpu::TextureUsages::empty(),
            label: None,
        };
        futures::executor::block_on(async {
            let res = generate_test(&texture_descriptor).await;
            assert!(res.is_err());
            assert!(res.err() == Some(Error::UnsupportedUsage(texture_descriptor.usage)));
        });
    }
}

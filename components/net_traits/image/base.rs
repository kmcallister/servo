/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use piston_image::{self, DynamicImage, GenericImage};
use util::vec::byte_swap;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Format {
    RGBA,
    Luma,
}

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub format: Format,
    pub data: Vec<u8>,
}

// TODO(pcwalton): Speed up with SIMD, or better yet, find some way to not do this.
fn byte_swap_and_premultiply(data: &mut [u8]) {
    let length = data.len();
    for i in (0..length).step_by(4) {
        let r = data[i + 2];
        let g = data[i + 1];
        let b = data[i + 0];
        let a = data[i + 3];
        data[i + 0] = ((r as u32) * (a as u32) / 255) as u8;
        data[i + 1] = ((g as u32) * (a as u32) / 255) as u8;
        data[i + 2] = ((b as u32) * (a as u32) / 255) as u8;
    }
}

pub fn load_from_memory(buffer: &[u8]) -> Option<Image> {
    if buffer.len() == 0 {
        return None;
    }

    // FIXME(#3144, #5371): This uses piston_image's "educated guess" about the
    // file format. We should use the MIME type from the Content-Type header,
    // and fall back to more sophisticated MIME sniffing.
    let image = match piston_image::load_from_memory(buffer) {
        Ok(i) => i,
        Err(_) => return None,
    };

    let (width, height) = image.dimensions();
    let (format, mut data) = match image {
        DynamicImage::ImageRgba8(buffer) => {
            let mut buffer = buffer.into_raw();
            byte_swap_and_premultiply(&mut buffer);
            (Format::RGBA, buffer)
        }

        image @ DynamicImage::ImageRgb8(_) => {
            let mut buffer = image.to_rgba().into_raw();
            byte_swap(&mut buffer);
            (Format::RGBA, buffer)
        }

        DynamicImage::ImageLuma8(buffer) => {
            (Format::Luma, buffer.into_raw())
        }

        image @ DynamicImage::ImageLumaA8(_) => {
            (Format::Luma, image.to_luma().into_raw())
        }
    };

    if let Format::Luma = format {
        // Invert greyscale
        for v in data.iter_mut() {
            *v = 255 - *v;
        }
    }

    Some(Image {
        width: width,
        height: height,
        format: format,
        data: data,
    })
}



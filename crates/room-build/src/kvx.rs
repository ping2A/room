//! Build KVX voxel model resources.
//!
//! The parser retains the compact column/slab representation and embedded
//! palette. It validates all mip levels before exposing the first for display.

use crate::BuildError;

const KVX_PALETTE_BYTES: usize = 256 * 3;
const MAX_KVX_BYTES: usize = 64 * 1024 * 1024;
const MAX_KVX_DIMENSION: usize = 256;
const MAX_KVX_MIP_LEVELS: usize = 16;

/// Parsed KVX model, including every encoded mip level and its VGA palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvxDocument {
    /// Mip levels in file order, from highest to lowest detail.
    pub mipmaps: Vec<KvxMipLevel>,
    /// Embedded 256-color RGB palette (component values are stored as bytes).
    pub palette: [u8; KVX_PALETTE_BYTES],
}

/// One KVX mip level represented as X/Y columns of vertical color slabs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvxMipLevel {
    /// Voxel dimensions in X, Y, Z order.
    pub dimensions: [u16; 3],
    /// Model pivot in signed 8.8 fixed-point voxel units.
    pub pivot: [i32; 3],
    /// Surface voxel columns in X-major, then Y-major order.
    pub columns: Vec<Vec<KvxSlab>>,
}

/// A vertical run of palette indices in one X/Y voxel column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvxSlab {
    /// First Z coordinate in the run.
    pub z_top: u8,
    /// Build face-visibility flags from the slab header.
    pub visibility: u8,
    /// Palette indices from `z_top` upwards.
    pub colors: Vec<u8>,
}

impl KvxDocument {
    /// Parse and validate a KVX model.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized, truncated, malformed, or internally
    /// inconsistent files. No offsets are followed until they have been
    /// checked against the containing mip level.
    pub fn parse(bytes: &[u8]) -> Result<Self, BuildError> {
        if bytes.len() > MAX_KVX_BYTES {
            return Err(BuildError::Invalid("KVX file size"));
        }
        let palette_start = bytes
            .len()
            .checked_sub(KVX_PALETTE_BYTES)
            .ok_or(BuildError::Truncated("KVX palette"))?;
        let mut position = 0usize;
        let mut mipmaps = Vec::new();
        while position < palette_start {
            if mipmaps.len() >= MAX_KVX_MIP_LEVELS {
                return Err(BuildError::Invalid("KVX mip level count"));
            }
            let (mipmap, next) = parse_mipmap(bytes, position, palette_start)?;
            mipmaps
                .try_reserve(1)
                .map_err(|_| BuildError::Resource("KVX mip allocation".into()))?;
            mipmaps.push(mipmap);
            position = next;
        }
        if position != palette_start || mipmaps.is_empty() {
            return Err(BuildError::Invalid("KVX mip level layout"));
        }

        let mut palette = [0u8; KVX_PALETTE_BYTES];
        palette.copy_from_slice(&bytes[palette_start..]);
        Ok(Self { mipmaps, palette })
    }
}

impl KvxMipLevel {
    /// Number of surface voxels stored in this level's slab records.
    #[must_use]
    pub fn surface_voxel_count(&self) -> usize {
        self.columns
            .iter()
            .flat_map(|column| column.iter())
            .map(|slab| slab.colors.len())
            .sum()
    }
}

fn parse_mipmap(
    bytes: &[u8],
    start: usize,
    palette_start: usize,
) -> Result<(KvxMipLevel, usize), BuildError> {
    let declared_bytes = usize::try_from(read_u32(bytes, start)?)
        .map_err(|_| BuildError::Invalid("KVX mip level size"))?;
    let end = start
        .checked_add(4)
        .and_then(|header| header.checked_add(declared_bytes))
        .filter(|end| *end <= palette_start)
        .ok_or(BuildError::Truncated("KVX mip level"))?;
    if declared_bytes < 24 {
        return Err(BuildError::Invalid("KVX mip header size"));
    }

    let dimensions = [
        read_u32(bytes, start + 4)?,
        read_u32(bytes, start + 8)?,
        read_u32(bytes, start + 12)?,
    ];
    let dims = dimensions
        .map(|value| usize::try_from(value).map_err(|_| BuildError::Invalid("KVX dimensions")));
    let [x_size, y_size, z_size] = dims;
    let [x_size, y_size, z_size] = [x_size?, y_size?, z_size?];
    if x_size == 0
        || y_size == 0
        || z_size == 0
        || x_size > MAX_KVX_DIMENSION
        || y_size > MAX_KVX_DIMENSION
        || z_size > MAX_KVX_DIMENSION
    {
        return Err(BuildError::Invalid("KVX dimensions"));
    }

    let pivot = [
        read_i32(bytes, start + 16)?,
        read_i32(bytes, start + 20)?,
        read_i32(bytes, start + 24)?,
    ];
    let x_offset_count = x_size
        .checked_add(1)
        .ok_or(BuildError::Invalid("KVX offset table size"))?;
    let xy_offset_count = x_size
        .checked_mul(
            y_size
                .checked_add(1)
                .ok_or(BuildError::Invalid("KVX offset table size"))?,
        )
        .ok_or(BuildError::Invalid("KVX offset table size"))?;
    let x_table_bytes = x_offset_count
        .checked_mul(4)
        .ok_or(BuildError::Invalid("KVX offset table size"))?;
    let xy_table_bytes = xy_offset_count
        .checked_mul(2)
        .ok_or(BuildError::Invalid("KVX offset table size"))?;
    let table_bytes = x_table_bytes
        .checked_add(xy_table_bytes)
        .ok_or(BuildError::Invalid("KVX offset table size"))?;
    let required_bytes = 24usize
        .checked_add(table_bytes)
        .ok_or(BuildError::Invalid("KVX mip level size"))?;
    if declared_bytes < required_bytes {
        return Err(BuildError::Truncated("KVX offset tables"));
    }

    let tables_start = start + 28;
    let mut x_offsets = Vec::new();
    x_offsets
        .try_reserve_exact(x_offset_count)
        .map_err(|_| BuildError::Resource("KVX offset allocation".into()))?;
    for index in 0..x_offset_count {
        x_offsets.push(
            usize::try_from(read_u32(bytes, tables_start + index * 4)?)
                .map_err(|_| BuildError::Invalid("KVX x offset"))?,
        );
    }
    if x_offsets.first().copied() != Some(table_bytes) {
        return Err(BuildError::Invalid("KVX first x offset"));
    }

    let xy_table_start = tables_start + x_table_bytes;
    let mut xy_offsets = Vec::new();
    xy_offsets
        .try_reserve_exact(xy_offset_count)
        .map_err(|_| BuildError::Resource("KVX offset allocation".into()))?;
    for index in 0..xy_offset_count {
        xy_offsets.push(usize::from(read_u16(bytes, xy_table_start + index * 2)?));
    }

    let slab_start = xy_table_start + xy_table_bytes;
    let slab_bytes = end - slab_start;
    let slab_end_offset = table_bytes
        .checked_add(slab_bytes)
        .ok_or(BuildError::Invalid("KVX slab data size"))?;
    if x_offsets.last().copied().unwrap_or_default() != slab_end_offset
        || x_offsets.windows(2).any(|pair| pair[0] > pair[1])
        || x_offsets.iter().any(|offset| *offset > slab_end_offset)
    {
        return Err(BuildError::Invalid("KVX x offsets"));
    }

    let column_count = x_size
        .checked_mul(y_size)
        .ok_or(BuildError::Invalid("KVX column count"))?;
    let mut columns = Vec::new();
    columns
        .try_reserve_exact(column_count)
        .map_err(|_| BuildError::Resource("KVX column allocation".into()))?;
    for x in 0..x_size {
        let x_start = x_offsets[x];
        let x_end = x_offsets[x + 1];
        let first_xy = x * (y_size + 1);
        if xy_offsets[first_xy] != 0
            || xy_offsets[first_xy + y_size] != x_end.saturating_sub(x_start)
            || xy_offsets[first_xy..=first_xy + y_size]
                .windows(2)
                .any(|pair| pair[0] > pair[1])
        {
            return Err(BuildError::Invalid("KVX y offsets"));
        }
        for y in 0..y_size {
            let absolute_start = x_start
                .checked_add(xy_offsets[first_xy + y])
                .ok_or(BuildError::Invalid("KVX column offset"))?;
            let absolute_end = x_start
                .checked_add(xy_offsets[first_xy + y + 1])
                .ok_or(BuildError::Invalid("KVX column offset"))?;
            let local_start = absolute_start
                .checked_sub(table_bytes)
                .ok_or(BuildError::Invalid("KVX column offset"))?;
            let local_end = absolute_end
                .checked_sub(table_bytes)
                .filter(|end| *end <= slab_bytes)
                .ok_or(BuildError::Invalid("KVX column offset"))?;
            let column_data = bytes
                .get(slab_start + local_start..slab_start + local_end)
                .ok_or(BuildError::Truncated("KVX column"))?;
            columns.push(parse_column(column_data, z_size)?);
        }
    }

    Ok((
        KvxMipLevel {
            dimensions: [
                u16::try_from(x_size).map_err(|_| BuildError::Invalid("KVX dimensions"))?,
                u16::try_from(y_size).map_err(|_| BuildError::Invalid("KVX dimensions"))?,
                u16::try_from(z_size).map_err(|_| BuildError::Invalid("KVX dimensions"))?,
            ],
            pivot,
            columns,
        },
        end,
    ))
}

fn parse_column(data: &[u8], z_size: usize) -> Result<Vec<KvxSlab>, BuildError> {
    let mut slabs = Vec::new();
    let mut position = 0usize;
    let mut previous_z_end = 0usize;
    while position < data.len() {
        let header_end = position
            .checked_add(3)
            .filter(|end| *end <= data.len())
            .ok_or(BuildError::Truncated("KVX slab header"))?;
        let z_top = usize::from(data[position]);
        let color_count = usize::from(data[position + 1]);
        let visibility = data[position + 2];
        if color_count == 0 || z_top < previous_z_end {
            return Err(BuildError::Invalid("KVX slab range"));
        }
        let colors_end = header_end
            .checked_add(color_count)
            .filter(|end| *end <= data.len())
            .ok_or(BuildError::Truncated("KVX slab colors"))?;
        let z_end = z_top
            .checked_add(color_count)
            .filter(|end| *end <= z_size)
            .ok_or(BuildError::Invalid("KVX slab height"))?;
        let mut colors = Vec::new();
        colors
            .try_reserve_exact(color_count)
            .map_err(|_| BuildError::Resource("KVX slab allocation".into()))?;
        colors.extend_from_slice(&data[header_end..colors_end]);
        slabs
            .try_reserve(1)
            .map_err(|_| BuildError::Resource("KVX slab allocation".into()))?;
        slabs.push(KvxSlab {
            z_top: u8::try_from(z_top).map_err(|_| BuildError::Invalid("KVX slab top"))?,
            visibility,
            colors,
        });
        previous_z_end = z_end;
        position = colors_end;
    }
    Ok(slabs)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, BuildError> {
    let field = bytes
        .get(offset..offset + 2)
        .ok_or(BuildError::Truncated("KVX u16"))?;
    Ok(u16::from_le_bytes([field[0], field[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, BuildError> {
    let field = bytes
        .get(offset..offset + 4)
        .ok_or(BuildError::Truncated("KVX u32"))?;
    Ok(u32::from_le_bytes([field[0], field[1], field[2], field[3]]))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, BuildError> {
    let field = bytes
        .get(offset..offset + 4)
        .ok_or(BuildError::Truncated("KVX i32"))?;
    Ok(i32::from_le_bytes([field[0], field[1], field[2], field[3]]))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn single_voxel() -> Vec<u8> {
        // One column table (4 + 4 bytes), one slab and a 256-color palette.
        let mut bytes = Vec::new();
        let numbytes = 24 + 8 + 4 + 4;
        bytes.extend_from_slice(&(numbytes as u32).to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&0i32.to_le_bytes());
        bytes.extend_from_slice(&12u32.to_le_bytes());
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 1, 0x3f, 7]);
        bytes.extend_from_slice(&[2; KVX_PALETTE_BYTES]);
        bytes
    }

    #[test]
    fn parses_slab_columns_and_embedded_palette() {
        let parsed = KvxDocument::parse(&single_voxel()).unwrap();
        assert_eq!(parsed.mipmaps.len(), 1);
        assert_eq!(parsed.mipmaps[0].dimensions, [1, 1, 1]);
        assert_eq!(parsed.mipmaps[0].pivot, [0, 0, 0]);
        assert_eq!(parsed.mipmaps[0].surface_voxel_count(), 1);
        assert_eq!(parsed.mipmaps[0].columns[0][0].colors, [7]);
        assert_eq!(parsed.mipmaps[0].columns[0][0].visibility, 0x3f);
        assert_eq!(parsed.palette, [2; KVX_PALETTE_BYTES]);
    }

    #[test]
    fn rejects_offsets_that_escape_the_level() {
        let mut bytes = single_voxel();
        bytes[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(KvxDocument::parse(&bytes).is_err());
    }

    #[test]
    fn rejects_truncated_or_out_of_bounds_slabs() {
        let mut truncated = single_voxel();
        truncated.remove(40);
        assert!(KvxDocument::parse(&truncated).is_err());

        let mut too_tall = single_voxel();
        let slab_top_offset = 4 + 24 + 8 + 4;
        too_tall[slab_top_offset] = 1;
        assert!(KvxDocument::parse(&too_tall).is_err());
    }
}

use std::{convert::TryInto, fmt::Debug};

use building_blocks::{
    core::prelude::*,
    mesh::{
        greedy_quads, padded_greedy_quads_chunk_extent, GreedyQuadsBuffer, IsOpaque, MergeVoxel,
        RIGHT_HANDED_Y_UP_CONFIG,
    },
    storage::prelude::*,
};

use crate::{format::Shape, Palette, PaletteIndex};

impl<'a> Shape<'a> {
    #[must_use]
    pub fn to_mesh(&self, palettes: &[Palette]) -> (Array3x1<PaletteIndex>, GreedyQuadsBuffer) {
        let size: [i32; 3] = self
            .voxels
            .size
            .map(|dim| dim.try_into().expect("shape size too large"));
        let extent = padded_greedy_quads_chunk_extent(&ExtentN {
            minimum: PointN([0, 0, 0]),
            shape: PointN(size),
        });
        let mut array = Array3x1::fill(extent, PaletteIndex(0, false));
        #[rustfmt::skip]
        let is_glass = match palettes.get(self.palette as usize) {
            Some(palette) => palette.materials.iter()
                .map(|material| material.rgba.0[3] < 1.0)
                .collect::<Vec<_>>().into_fixed(),
            None => [false; 256],
        };
        for (coord, palette_index) in self.iter_voxels() {
            *array.get_mut(PointN(coord)) =
                PaletteIndex(palette_index, is_glass[palette_index as usize]);
        }
        let mut buffer = GreedyQuadsBuffer::new(extent, RIGHT_HANDED_Y_UP_CONFIG.quad_groups());
        greedy_quads(&array, &extent, &mut buffer);
        (array, buffer)
    }
}

impl MergeVoxel for PaletteIndex {
    type VoxelValue = u8;

    fn voxel_merge_value(&self) -> Self::VoxelValue {
        self.0
    }
}

impl IsEmpty for PaletteIndex {
    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl IsOpaque for PaletteIndex {
    fn is_opaque(&self) -> bool {
        !self.1
    }
}

// Duplicate in editor-format
trait IntoFixedArray {
    type Item;

    fn into_fixed<const N: usize>(self) -> [Self::Item; N];
}

impl<T: Debug> IntoFixedArray for Vec<T> {
    type Item = T;

    fn into_fixed<const N: usize>(self) -> [Self::Item; N] {
        #[allow(clippy::unwrap_used)]
        self.try_into().unwrap()
    }
}

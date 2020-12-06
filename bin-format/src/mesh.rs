use std::convert::TryInto;
use building_blocks::{
    core::prelude::*,
    mesh::{greedy_quads, padded_greedy_quads_chunk_extent, GreedyQuadsBuffer, IsOpaque, MergeVoxel},
    storage::prelude::*,
};
use crate::{PaletteIndex, format::Shape};

impl<'a> Shape<'a> {
    pub fn to_mesh(&self) -> (Array3<PaletteIndex>, GreedyQuadsBuffer) {
        let size: [i32; 3] = self.voxel_data.size.map(|dim| dim.try_into().expect("shape size too large"));
        let extent = padded_greedy_quads_chunk_extent(&ExtentN {
            minimum: PointN([0, 0, 0]),
            shape: PointN(size)
        });
        let mut array = Array3::fill(extent, PaletteIndex(0));
        for (coord, palette_index) in self.iter_voxels() {
            *array.get_mut(PointN(coord)) = PaletteIndex(palette_index);
        }
        let mut buffer = GreedyQuadsBuffer::new(extent);
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
        // Checks if it is glass. Should check alpha instead.
        if self.0 == 0 || (self.0 - 1) / 8 == 0 {
            false
        } else { true }
    }
}


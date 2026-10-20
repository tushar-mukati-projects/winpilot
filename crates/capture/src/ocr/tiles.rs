use crate::pixels::Block;

pub(super) fn tiles(width: u32, height: u32, side: u32, overlap: u32) -> Vec<Block> {
    let columns = tile_starts(width, side, overlap);
    tile_starts(height, side, overlap)
        .into_iter()
        .flat_map(|y| {
            columns.iter().map(move |&x| Block { x, y, width: side.min(width - x), height: side.min(height - y) })
        })
        .collect()
}

fn tile_starts(length: u32, side: u32, overlap: u32) -> Vec<u32> {
    let step = side.saturating_sub(overlap).max(1);
    let mut starts = vec![0];
    let mut start = 0;
    while start + side < length {
        start += step;
        starts.push(start);
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_the_frame_with_overlap() {
        let tiles = tiles(5000, 1600, 1300, 64);
        assert_eq!(tiles.len(), 8);
        assert_eq!(tiles.iter().map(|tile| tile.x + tile.width).max(), Some(5000));
        assert_eq!(tiles.iter().map(|tile| tile.y + tile.height).max(), Some(1600));
        assert_eq!(tiles[1], Block { x: 1236, y: 0, width: 1300, height: 1300 });
        assert_eq!(super::tiles(800, 600, 1300, 64), vec![Block { x: 0, y: 0, width: 800, height: 600 }]);
    }
}

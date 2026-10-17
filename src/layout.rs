// Shared by row rendering and keyboard paging.
pub const ARTIST_ROW_HEIGHT: f32 = 56.;
pub const ALBUM_ROW_HEIGHT: f32 = 64.;
pub const SONG_ROW_HEIGHT: f32 = 56.;
pub const FOLDER_ROW_HEIGHT: f32 = 46.;
pub const PLAYER_HEIGHT: f32 = 86.;
pub const SIDEBAR_COMPACT_WIDTH: f32 = 64.;
pub const SIDEBAR_EXPANDED_WIDTH: f32 = 208.;

pub fn page_items(viewport_height: f32, row_height: f32) -> usize {
    (viewport_height / row_height).floor().max(1.) as usize
}

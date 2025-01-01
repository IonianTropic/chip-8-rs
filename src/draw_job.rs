#[derive(Debug)]
pub enum DrawJob {
    Draw(Sprite),
    Clear,
}

#[derive(Debug)]
pub struct Sprite {
    pub v_x: usize,
    pub v_y: usize,
    pub buf: Vec<u8>,
}

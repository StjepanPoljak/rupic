pub struct ByteDataBlock {
    pub address: usize,
    pub bytes: Vec<u8>
}

pub struct ByteData {
    data_blocks: Vec<ByteDataBlock>
}

impl ByteData {
    pub fn new() -> Self {
        Self { data_blocks : vec![] }
    }

    pub fn push(&mut self, data: ByteDataBlock) {
        self.data_blocks.push(data);
    }

    pub fn iter(&self) -> std::slice::Iter<'_, ByteDataBlock> {
        self.data_blocks.iter()
    }
}

impl<'a> IntoIterator for &'a ByteData {
    type Item = &'a ByteDataBlock;
    type IntoIter = std::slice::Iter<'a, ByteDataBlock>;

    fn into_iter(self) -> Self::IntoIter {
        self.data_blocks.iter()
    }
}

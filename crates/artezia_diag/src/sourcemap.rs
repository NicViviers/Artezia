use std::path::{Path, PathBuf};

type Span = std::ops::Range<usize>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub u32);

pub struct SourceFile {
    pub name: PathBuf,
    pub base: usize, // Start offset in the global space
    pub len: usize
}

#[derive(Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
    pub text: String // All files concatenated - the `src` the whole compiler sees
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a file and return it's ID. The file's span must be offset by `base`
    pub fn add(&mut self, name: PathBuf, text: &str) -> FileId {
        let base = self.text.len();
        self.text.push_str(text);

        self.text.push('\n'); // Separtor keeps adjacent files from merging token-wise if anything ever elxes the concatenation directly

        let id = FileId(self.files.len() as u32);
        self.files.push(SourceFile {
            name,
            base,
            len: text.len()
        });

        id
    }

    pub fn base_of(&self, id: FileId) -> usize {
        self.files[id.0 as usize].base
    }

    /// Which file does this global offset belong to
    pub fn file_of(&self, offset: usize) -> FileId {
        let idx = self.files.partition_point(|f| f.base <= offset).saturating_sub(1);
        FileId(idx as u32)
    }

    pub fn name_of(&self, id: FileId) -> &Path {
        &self.files[id.0 as usize].name
    }

    /// (file, line, column) for a global offset - diagnostics only
    pub fn line_col(&self, offset: usize) -> (FileId, u32, u32) {
        let fid = self.file_of(offset);
        let f = &self.files[fid.0 as usize];
        let local = offset - f.base;
        let text = &self.text[f.base .. f.base + f.len];
        let mut line = 1u32;
        let mut col = 1u32;

        for (i, ch) in text.char_indices() {
            if i >= local { break; }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }

        (fid, line, col)
    }

    /// (display name, file text) for every loaded file
    pub fn sources(&self) -> impl Iterator<Item = (String, &str)> + '_ {
        self.files.iter().map(|f| (
            f.name.display().to_string(),
            &self.text[f.base .. f.base + f.len]
        ))
    }

    pub fn text_of(&self, id: FileId) -> &str {
        let f = &self.files[id.0 as usize];
        &self.text[f.base .. f.base + f.len]
    }
}

pub fn localize(map: &SourceMap, span: &Span) -> (String, std::ops::Range<usize>) {
    let fid = map.file_of(span.start);
    let base = map.base_of(fid);
    let name = map.name_of(fid).display().to_string();
    (name, (span.start - base) .. (span.end - base))
}
use artezia_diag::*;

#[test]
fn add_returns_sequential_ids_and_bases() {
    let mut m = SourceMap::new();
    let a = m.add("a.tia".into(), "abc");
    let b = m.add("b.tia".into(), "defg");

    assert_eq!(m.base_of(a), 0);
    // "abc" (3) + the '\n' separator = 4
    assert_eq!(m.base_of(b), 4);
    assert_ne!(a, b);
}

#[test]
fn file_of_maps_offsets_to_the_right_file() {
    let mut m = SourceMap::new();
    let a = m.add("a.tia".into(), "abc");
    let b = m.add("b.tia".into(), "defg");
    let bb = m.base_of(b);

    assert_eq!(m.file_of(0), a); // First byte of a
    assert_eq!(m.file_of(2), a); // Last byte of a
    assert_eq!(m.file_of(bb), b); // First byte of b
    assert_eq!(m.file_of(bb + 3), b); // Last byte of b
}

#[test]
fn name_of_and_text_of_round_trip() {
    let mut m = SourceMap::new();
    let a = m.add("a.tia".into(), "abc");
    let b = m.add("b.tia".into(), "defg");

    assert_eq!(m.name_of(a).to_string_lossy(), "a.tia");
    assert_eq!(m.text_of(a), "abc");
    assert_eq!(m.text_of(b), "defg"); // Separator not included
}

#[test]
fn line_col_counts_lines_within_a_file() {
    let mut m = SourceMap::new();
    let a = m.add("a.tia".into(), "one\ntwo\nthree");

    // Offset 0 = 'o' of "one"
    assert_eq!(m.line_col(0), (a, 1, 1));
    // Offset 4 = 't' of "two" (after "one\n")
    assert_eq!(m.line_col(4), (a, 2, 1));
    // Offset 6 = 'o' of "two"
    assert_eq!(m.line_col(6), (a, 2, 3));
    // Offset 8 = 't' of "three"
    assert_eq!(m.line_col(8), (a, 3, 1));
}

#[test]
fn line_col_is_file_relative_not_global() {
    let mut m = SourceMap::new();
    m.add("a.tia".into(), "one\ntwo");
    let b = m.add("b.tia".into(), "xyz");
    let bb = m.base_of(b);

    // First char of b is line 1 col 1 OF B, despite a large global offset
    assert_eq!(m.line_col(bb), (b, 1, 1));
    assert_eq!(m.line_col(bb + 2), (b, 1, 3));
}

#[test]
fn single_file_behaves_like_no_map_at_all() {
    // One file, base 0, offsets pass through unchanged
    let mut m = SourceMap::new();
    let a = m.add("only.tia".into(), "func main() {}");
    assert_eq!(m.base_of(a), 0);
    assert_eq!(m.file_of(5), a);
    assert_eq!(m.line_col(0), (a, 1, 1));
}
use aidget::runtime::ui::distinct_names;

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn twin_displays_get_different_names() {
    assert_eq!(
        distinct_names(names(&["Monitor #7", "Monitor #7", "Monitor #7"])),
        names(&["Monitor #7", "Monitor #7 (2)", "Monitor #7 (3)"])
    );
}

#[test]
fn different_displays_keep_their_names_so_saved_frames_stay_valid() {
    assert_eq!(
        distinct_names(names(&["Monitor #1", "Monitor #2"])),
        names(&["Monitor #1", "Monitor #2"])
    );
}

use super::*;

fn press(input: &mut TextInput, code: KeyCode, modifiers: KeyModifiers) {
    assert!(input.key(KeyEvent::new(code, modifiers), 100), "{code:?}");
}

fn type_in(input: &mut TextInput, text: &str) {
    for c in text.chars() {
        press(input, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

#[test]
fn typing_goes_in_at_the_cursor() {
    let mut t = TextInput::new("helo");
    press(&mut t, KeyCode::Left, KeyModifiers::NONE);
    type_in(&mut t, "l");
    assert_eq!((t.text(), t.cursor()), ("hello", 4));
    press(&mut t, KeyCode::Home, KeyModifiers::NONE);
    type_in(&mut t, ">");
    assert_eq!((t.text(), t.cursor()), (">hello", 1));
    press(&mut t, KeyCode::End, KeyModifiers::NONE);
    type_in(&mut t, "!");
    assert_eq!(t.text(), ">hello!");
}

#[test]
fn arrows_stop_at_the_ends() {
    let mut t = TextInput::new("ab");
    for _ in 0..5 {
        press(&mut t, KeyCode::Right, KeyModifiers::NONE);
    }
    assert_eq!(t.cursor(), 2);
    for _ in 0..5 {
        press(&mut t, KeyCode::Left, KeyModifiers::NONE);
    }
    assert_eq!(t.cursor(), 0);
}

#[test]
fn backspace_and_delete_work_at_the_cursor() {
    let mut t = TextInput::new("abcd");
    press(&mut t, KeyCode::Left, KeyModifiers::NONE);
    press(&mut t, KeyCode::Left, KeyModifiers::NONE);
    press(&mut t, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!((t.text(), t.cursor()), ("acd", 1));
    press(&mut t, KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!((t.text(), t.cursor()), ("ad", 1));
    press(&mut t, KeyCode::Home, KeyModifiers::NONE);
    press(&mut t, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(t.text(), "ad", "nothing before the cursor");
    press(&mut t, KeyCode::End, KeyModifiers::NONE);
    press(&mut t, KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!(t.text(), "ad", "nothing after the cursor");
}

#[test]
fn ctrl_keys_move_and_clear() {
    let mut t = TextInput::new("one two");
    press(&mut t, KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert_eq!(t.cursor(), 0);
    press(&mut t, KeyCode::Char('e'), KeyModifiers::CONTROL);
    assert_eq!(t.cursor(), 7);
    press(&mut t, KeyCode::Char('w'), KeyModifiers::CONTROL);
    assert_eq!(t.text(), "one ");
    press(&mut t, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!((t.text(), t.cursor()), ("", 0));
}

#[test]
fn multibyte_characters_count_as_one() {
    let mut t = TextInput::new("añb");
    press(&mut t, KeyCode::Left, KeyModifiers::NONE);
    press(&mut t, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(t.text(), "ab");
    press(&mut t, KeyCode::Home, KeyModifiers::NONE);
    type_in(&mut t, "名");
    assert_eq!((t.text(), t.cursor()), ("名ab", 1));
}

#[test]
fn the_length_limit_stops_typing_only() {
    let mut t = TextInput::new("abc");
    assert!(t.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE), 3));
    assert_eq!(t.text(), "abc");
    press(&mut t, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(t.text(), "ab");
}

#[test]
fn enter_and_esc_are_left_to_the_caller() {
    let mut t = TextInput::new("x");
    assert!(!t.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 9));
    assert!(!t.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), 9));
}

#[test]
fn the_window_keeps_the_cursor_in_view() {
    let one = |_: char| 1;
    let t = TextInput::new("abcdefghij");
    assert_eq!(t.window(20, false, one), ("abcdefghij".into(), 10));
    // Cursor at the end of a long line: the tail shows, the cursor after it.
    assert_eq!(t.window(5, false, one), ("ghij".into(), 4));
    let mut moved = t.clone();
    for _ in 0..10 {
        moved.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), 99);
    }
    assert_eq!(moved.window(5, false, one), ("abcde".into(), 0));
    assert_eq!(TextInput::new("abc").window(8, true, one), ("•••".into(), 3));
}

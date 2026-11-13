use super::*;

#[test]
fn the_memory_keychain_keeps_one_password_per_account() {
    let k = MemoryKeychain::new();
    assert_eq!(k.get("a@x.com"), Ok(None));
    k.set("a@x.com", "one").unwrap();
    k.set("b@x.com", "two").unwrap();
    k.set("a@x.com", "three").unwrap();
    assert_eq!(k.get("a@x.com"), Ok(Some("three".into())), "replaced");
    assert_eq!(k.get("b@x.com"), Ok(Some("two".into())));
    k.forget("a@x.com").unwrap();
    assert_eq!(k.get("a@x.com"), Ok(None));
    k.forget("a@x.com").unwrap();
    assert!(k.available());
}

#[test]
fn a_missing_keychain_fails_everything_and_says_it_is_not_there() {
    let k = MemoryKeychain::missing();
    assert!(!k.available());
    assert!(k.get("a").is_err() && k.set("a", "p").is_err() && k.forget("a").is_err());
}

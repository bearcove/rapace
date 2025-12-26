//! Tests for zero-copy deserialization with OwnedMessage.
//!
//! NOTE: Some tests are `#[ignore]` because facet-format-postcard doesn't yet support
//! deserialization of borrowed types (`&'a [u8]`, `Cow<'a, str>`). The OwnedMessage
//! infrastructure is ready - we're waiting on upstream facet support.
//!
//! Once facet-format-postcard supports borrowed types, remove the `#[ignore]` attributes.

use rapace::rapace_core::{Frame, MsgDescHot, OwnedMessage, Payload};
use std::borrow::Cow;

/// A type with a lifetime that can borrow from the input - byte slice only.
#[derive(Debug, PartialEq, facet::Facet)]
struct BytesOnly<'a> {
    data: &'a [u8],
    count: u32,
}

/// A type with just Cow<str> - no raw slices.
#[derive(Debug, PartialEq, facet::Facet)]
struct CowOnly<'a> {
    message: Cow<'a, str>,
    count: u32,
}

/// A type with a lifetime that can borrow from the input.
#[derive(Debug, PartialEq, facet::Facet)]
struct BorrowingResponse<'a> {
    message: Cow<'a, str>,
    data: &'a [u8],
}

/// A simple owned type for comparison.
#[derive(Debug, PartialEq, facet::Facet)]
struct OwnedResponse {
    message: String,
    count: u32,
}

fn make_frame(payload: &[u8]) -> Frame {
    let mut desc = MsgDescHot::new();
    desc.payload_len = payload.len() as u32;
    Frame {
        desc,
        payload: Payload::Owned(payload.to_vec()),
    }
}

#[test]
#[ignore = "facet-format-postcard doesn't yet support borrowed types - see facet-rs/facet#1474"]
fn test_owned_message_with_borrowing_type() {
    // Serialize a borrowing response
    let original = BorrowingResponse {
        message: Cow::Borrowed("hello world"),
        data: b"binary data",
    };

    let bytes = rapace::facet_format_postcard::to_vec(&original).expect("serialize");
    let frame = make_frame(&bytes);

    // Deserialize using OwnedMessage - this should borrow from the frame
    let owned: OwnedMessage<BorrowingResponse<'static>> = OwnedMessage::try_new(frame, |payload| {
        rapace::facet_format_postcard::from_slice(payload)
    })
    .expect("deserialize");

    // Access via Deref
    assert_eq!(&*owned.message, "hello world");
    assert_eq!(owned.data, b"binary data");

    // Verify it's actually borrowed (Cow::Borrowed), not owned
    assert!(matches!(owned.message, Cow::Borrowed(_)));
}

#[test]
#[ignore = "facet-format-postcard doesn't yet support borrowed types - see facet-rs/facet#1474"]
fn test_owned_message_recovers_frame() {
    let original = BorrowingResponse {
        message: Cow::Borrowed("test"),
        data: b"data",
    };

    let bytes = rapace::facet_format_postcard::to_vec(&original).expect("serialize");
    let frame = make_frame(&bytes);
    let original_len = frame.payload_bytes().len();

    let owned: OwnedMessage<BorrowingResponse<'static>> = OwnedMessage::try_new(frame, |payload| {
        rapace::facet_format_postcard::from_slice(payload)
    })
    .expect("deserialize");

    // Recover the frame
    let recovered = owned.into_frame();
    assert_eq!(recovered.payload_bytes().len(), original_len);
}

#[test]
#[ignore = "facet-format-postcard doesn't yet support borrowed types - see facet-rs/facet#1474"]
fn test_owned_message_debug() {
    let original = BorrowingResponse {
        message: Cow::Borrowed("debug test"),
        data: b"bytes",
    };

    let bytes = rapace::facet_format_postcard::to_vec(&original).expect("serialize");
    let frame = make_frame(&bytes);

    let owned: OwnedMessage<BorrowingResponse<'static>> = OwnedMessage::try_new(frame, |payload| {
        rapace::facet_format_postcard::from_slice(payload)
    })
    .expect("deserialize");

    let debug_str = format!("{:?}", owned);
    assert!(debug_str.contains("OwnedMessage"));
    assert!(debug_str.contains("debug test"));
}

/// Test with just &'a [u8] to see if basic borrowed slices work.
#[test]
#[ignore = "facet-format-postcard doesn't yet support borrowed types - see facet-rs/facet#1474"]
fn test_bytes_only() {
    let original = BytesOnly {
        data: b"hello bytes",
        count: 42,
    };

    let bytes = rapace::facet_format_postcard::to_vec(&original).expect("serialize");
    let frame = make_frame(&bytes);

    // Deserialize using OwnedMessage
    let owned: OwnedMessage<BytesOnly<'static>> = OwnedMessage::try_new(frame, |payload| {
        rapace::facet_format_postcard::from_slice(payload)
    })
    .expect("deserialize");

    assert_eq!(owned.data, b"hello bytes");
    assert_eq!(owned.count, 42);
}

/// Test with just Cow<'a, str> to see if Cow works.
#[test]
#[ignore = "facet-format-postcard doesn't yet support borrowed types - see facet-rs/facet#1474"]
fn test_cow_only() {
    let original = CowOnly {
        message: Cow::Borrowed("hello cow"),
        count: 99,
    };

    let bytes = rapace::facet_format_postcard::to_vec(&original).expect("serialize");
    let frame = make_frame(&bytes);

    // Deserialize using OwnedMessage
    let owned: OwnedMessage<CowOnly<'static>> = OwnedMessage::try_new(frame, |payload| {
        rapace::facet_format_postcard::from_slice(payload)
    })
    .expect("deserialize");

    assert_eq!(&*owned.message, "hello cow");
    assert_eq!(owned.count, 99);
}

/// Test OwnedMessage infrastructure with an owned type.
/// This verifies the wrapper works correctly even without zero-copy benefits.
#[test]
fn test_owned_message_with_owned_type() {
    let original = OwnedResponse {
        message: "test string".to_string(),
        count: 123,
    };

    let bytes = rapace::facet_format_postcard::to_vec(&original).expect("serialize");
    let frame = make_frame(&bytes);

    // OwnedMessage works with any type, even owned ones
    let owned: OwnedMessage<OwnedResponse> = OwnedMessage::try_new(frame, |payload| {
        rapace::facet_format_postcard::from_slice(payload)
    })
    .expect("deserialize");

    assert_eq!(owned.message, "test string");
    assert_eq!(owned.count, 123);

    // Verify Deref works
    let borrowed: &OwnedResponse = &owned;
    assert_eq!(borrowed.message, "test string");

    // Verify into_frame works
    let frame = owned.into_frame();
    assert_eq!(frame.payload_bytes().len(), bytes.len());
}

// This test verifies that the type_has_lifetime detection in the macro works correctly.
// Types without lifetimes should NOT trigger the zero-copy path.
#[test]
fn test_owned_type_still_works() {
    let original = OwnedResponse {
        message: "owned string".to_string(),
        count: 42,
    };

    let bytes = rapace::facet_format_postcard::to_vec(&original).expect("serialize");
    let frame = make_frame(&bytes);

    // For owned types, we just deserialize directly (no OwnedMessage needed)
    let result: OwnedResponse =
        rapace::facet_format_postcard::from_slice(frame.payload_bytes()).expect("deserialize");

    assert_eq!(result.message, "owned string");
    assert_eq!(result.count, 42);
}

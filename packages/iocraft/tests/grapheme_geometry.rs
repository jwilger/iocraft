use iocraft::prelude::*;
use iocraft::{Canvas, CanvasCell};

#[test]
fn text_intrinsic_size_retains_leading_mark_and_following_letter() {
    let canvas = element!(Text(content: "\u{301}a")).render(None);
    assert_eq!(canvas.width(), 2);
    assert_eq!(canvas.to_string(), " \u{301}a\n");
    let canvas = element!(Text(content: "\u{301}")).render(None);
    assert_eq!(canvas.width(), 1);
    assert_eq!(canvas.to_string(), " \u{301}\n");
}

#[test]
fn joined_emoji_is_painted_as_one_two_cell_grapheme() {
    let canvas = element!(Text(content: "👩\u{200d}💻x")).render(None);
    assert_eq!(canvas.width(), 3);
    assert_eq!(canvas.to_string(), "👩\u{200d}💻x\n");
    let mut ansi = Vec::new();
    canvas.write_ansi(&mut ansi).unwrap();
    assert!(String::from_utf8(ansi).unwrap().contains("👩\u{200d}💻"));
}

#[test]
fn following_sibling_starts_after_the_same_occupied_cells() {
    for (content, width, expected) in [
        ("\u{301}a", 2, " \u{301}a|\n"),
        ("👩\u{200d}💻", 2, "👩\u{200d}💻|\n"),
        ("界e\u{301}", 3, "界e\u{301}|\n"),
    ] {
        let canvas = element!(View(flex_direction: FlexDirection::Row) {
            Text(content: content)
            Text(content: "|", color: Color::Cyan)
        })
        .render(None);
        assert_eq!(canvas.to_string(), expected);
        assert_eq!(
            canvas
                .cell(width, 0)
                .and_then(CanvasCell::text_style)
                .and_then(|s| s.color),
            Some(Color::Cyan)
        );
    }
}

#[test]
fn forced_wrapping_keeps_graphemes_and_occupied_column_bounds() {
    let canvas = element!(View(width: 2, height: 4, flex_direction: FlexDirection::Column) {
        Text(content: "\u{301}ab👩\u{200d}💻x")
    })
    .render(None);
    assert_eq!(canvas.to_string(), " \u{301}a\nb\n👩\u{200d}💻\nx\n");
    assert_eq!(canvas.width(), 2);
}

#[test]
fn right_alignment_and_ansi_output_keep_a_standalone_mark_base() {
    let canvas = element!(View(width: 4, flex_direction: FlexDirection::Column) {
        Text(content: "\u{301}a", align: TextAlign::Right, wrap: TextWrap::NoWrap)
    })
    .render(None);
    assert_eq!(canvas.to_string(), "   \u{301}a\n");
    let mut ansi = Vec::new();
    canvas.write_ansi(&mut ansi).unwrap();
    let plain = String::from_utf8(ansi)
        .unwrap()
        .replace("\x1b[0m", "")
        .replace("\x1b[K", "")
        .replace('\r', "");
    assert_eq!(plain, canvas.to_string());
}

#[test]
fn canvas_clips_and_overwrites_complete_joined_graphemes() {
    let mut canvas = Canvas::new(6, 1);
    canvas
        .subview_mut(0, 0, 0, 0, 6, 1)
        .set_text(0, 0, "abcdef", Default::default());
    canvas
        .subview_mut(1, 0, 1, 0, 2, 1)
        .set_text(0, 0, "👩\u{200d}💻", Default::default());
    assert_eq!(canvas.to_string(), "a👩\u{200d}💻def\n");
    canvas
        .subview_mut(0, 0, 0, 0, 6, 1)
        .set_text(2, 0, "X", Default::default());
    assert_eq!(canvas.to_string(), "a Xdef\n");
    let mut clipped = Canvas::new(3, 1);
    clipped
        .subview_mut(0, 0, 1, 0, 2, 1)
        .set_text(0, 0, "👩\u{200d}💻Z", Default::default());
    assert_eq!(clipped.to_string(), "  Z\n");
}

#[test]
fn zero_width_suffixes_attach_without_reserving_another_cell() {
    for (content, width, expected) in [
        ("a\u{200b}", 1, "a\u{200b}\n"),
        ("a\u{200b}\u{200b}b", 2, "a\u{200b}\u{200b}b\n"),
        ("\u{200b}\u{301}a", 2, " \u{200b}\u{301}a\n"),
    ] {
        let canvas = element!(Text(content: content)).render(None);
        assert_eq!(canvas.width(), width, "{content:?}");
        assert_eq!(canvas.to_string(), expected);
    }
    for (content, expected) in [
        ("\u{301}a", " \u{301}\na\n"),
        ("a\u{200b}b", "a\u{200b}\nb\n"),
    ] {
        let canvas = element!(View(width: 1, height: 2, flex_direction: FlexDirection::Column) {
            Text(content: content)
        })
        .render(None);
        assert_eq!(canvas.to_string(), expected);
    }
}

#[test]
fn public_width_matches_intrinsic_text_cells() {
    for content in [
        "",
        "abc",
        "\u{301}",
        "\u{301}a",
        "👩\u{200d}💻",
        "界e\u{301}",
    ] {
        let canvas = element!(Text(content: content)).render(None);
        assert_eq!(Canvas::text_width(content), canvas.width());
    }
}

#[test]
fn wrapping_rows_agree_with_explicit_line_text_composition() {
    for (input, width, expected, painted) in [
        ("", 3, vec![""], "\n"),
        ("a\n", 3, vec!["a", ""], "a\n\n"),
        ("\na\n\n", 3, vec!["", "a", "", ""], "\na\n\n\n"),
        ("abcdefg", 3, vec!["abc", "def", "g"], "abc\ndef\ng\n"),
        (
            "\u{301}ab👩\u{200d}💻x",
            2,
            vec!["\u{301}a", "b", "👩\u{200d}💻", "x"],
            " \u{301}a\nb\n👩\u{200d}💻\nx\n",
        ),
    ] {
        let rows: Vec<String> = input
            .split('\n')
            .flat_map(|line| Text::wrap_lines(line, width))
            .collect();
        assert_eq!(rows, expected);
        let canvas = element!(View(width: width as u16, height: expected.len() as u16, flex_direction: FlexDirection::Column) {
            #(input.split('\n').map(|line| element!(Text(content: line))))
        }).render(None);
        assert_eq!(canvas.to_string(), painted);
    }
}

#[test]
fn wrapping_rows_strip_ansi_like_text_without_returning_styling() {
    let content = "\x1b[31mabcdef\x1b[0m";
    assert_eq!(Text::wrap_lines(content, 3), ["abc", "def"]);
    let canvas = element!(View(width: 3, height: 2, flex_direction: FlexDirection::Column) {
        Text(content: content)
    })
    .render(None);
    assert_eq!(canvas.to_string(), "abc\ndef\n");
    assert_eq!(
        canvas
            .cell(0, 0)
            .and_then(CanvasCell::text_style)
            .and_then(|s| s.color),
        None
    );
}

use std::io::{self, Write};

use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
use ratatui_ratty::{ObjectFormat, RattyGraphic, RattyGraphicSettings};

/// Stable, mtop-owned Ratty object IDs; these never clear unrelated RGP objects.
pub const PANEL_OBJECT_IDS: [u32; 6] = [
    0x4d54_0000,
    0x4d54_0001,
    0x4d54_0002,
    0x4d54_0003,
    0x4d54_0004,
    0x4d54_0005,
];

fn graphic(id: u32) -> RattyGraphic<'static> {
    RattyGraphic::new(
        RattyGraphicSettings::new("mtop-panel-frame.obj")
            .id(id)
            .format(ObjectFormat::Obj)
            .normalize(false)
            .animate(false)
            .depth(0.12)
            .color([73, 157, 255])
            .brightness(1.15),
    )
}

fn panel_frame_obj() -> Vec<u8> {
    // Four thin, extruded bars leave the terminal content visible through the
    // middle while giving each Ratatui panel a physical 3D bezel in Ratty.
    let bars = [
        [-1.0, 0.92, 1.0, 1.0],
        [-1.0, -1.0, 1.0, -0.92],
        [-1.0, -0.92, -0.94, 0.92],
        [0.94, -0.92, 1.0, 0.92],
    ];
    let mut obj = String::with_capacity(1200);
    let mut base = 1u32;
    for [x0, y0, x1, y1] in bars {
        let vertices = [
            [x0, y0, 0.0],
            [x1, y0, 0.0],
            [x1, y1, 0.0],
            [x0, y1, 0.0],
            [x0, y0, 0.12],
            [x1, y0, 0.12],
            [x1, y1, 0.12],
            [x0, y1, 0.12],
        ];
        for [x, y, z] in vertices {
            obj.push_str(&format!("v {x:.3} {y:.3} {z:.3}\n"));
        }
        for [a, b, c, d] in [
            [0, 1, 2, 3],
            [4, 7, 6, 5],
            [0, 4, 5, 1],
            [1, 5, 6, 2],
            [2, 6, 7, 3],
            [3, 7, 4, 0],
        ] {
            obj.push_str(&format!(
                "f {} {} {} {}\n",
                base + a,
                base + b,
                base + c,
                base + d
            ));
        }
        base += 8;
    }
    obj.into_bytes()
}

/// RGP registration commands for all panel-frame assets.
pub fn panel_registration_sequences() -> Vec<String> {
    let payload = panel_frame_obj();
    PANEL_OBJECT_IDS
        .iter()
        .flat_map(|id| {
            graphic(*id)
                .register_payload_sequences_with_name(&payload, Some("mtop-panel-frame.obj"))
        })
        .collect()
}

/// Register panel frames on a Ratty terminal. Call once before the first frame.
pub fn write_panel_registrations(mut writer: impl Write) -> io::Result<()> {
    for sequence in panel_registration_sequences() {
        writer.write_all(sequence.as_bytes())?;
    }
    writer.flush()
}

/// Placement command for one 3D bezel.
pub fn panel_placement_sequence(id: u32, area: Rect) -> String {
    graphic(id).place_sequence(area)
}

/// Delete command for one mtop-owned bezel.
pub fn panel_delete_sequence(id: u32) -> String {
    graphic(id).delete_sequence()
}

/// Emit frame placements for visible panel rectangles and return their count.
/// The upstream widget preserves the APC sequence's zero-width buffer diff.
pub fn emit_panel_frames(buffer: &mut Buffer, areas: &[Rect]) -> usize {
    let mut count = 0;
    for (id, area) in PANEL_OBJECT_IDS
        .iter()
        .zip(areas.iter().copied().filter(|area| !area.is_empty()))
    {
        if count == PANEL_OBJECT_IDS.len() {
            break;
        }
        (&graphic(*id)).render(area, buffer);
        count += 1;
    }
    count
}

/// Delete panel IDs no longer used after a layout transition.
pub fn write_panel_deletes(
    mut writer: impl Write,
    first_unused: usize,
    previously_visible: usize,
) -> io::Result<()> {
    for id in PANEL_OBJECT_IDS
        .iter()
        .skip(first_unused.min(PANEL_OBJECT_IDS.len()))
        .take(
            previously_visible
                .min(PANEL_OBJECT_IDS.len())
                .saturating_sub(first_unused),
        )
    {
        writer.write_all(panel_delete_sequence(*id).as_bytes())?;
    }
    writer.flush()
}

/// Remove all panel frames when the interactive dashboard exits.
pub fn write_panel_cleanup(mut writer: impl Write) -> io::Result<()> {
    for id in PANEL_OBJECT_IDS {
        writer.write_all(panel_delete_sequence(id).as_bytes())?;
    }
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::{
        PANEL_OBJECT_IDS, emit_panel_frames, panel_delete_sequence, panel_placement_sequence,
        panel_registration_sequences, write_panel_deletes,
    };
    use ratatui::{
        buffer::{Buffer, CellWidth},
        layout::Rect,
        style::Style,
    };

    #[test]
    fn panel_assets_register_with_distinct_ratty_object_ids() {
        let sequences = panel_registration_sequences();
        assert_eq!(sequences.len(), PANEL_OBJECT_IDS.len());
        for (id, sequence) in PANEL_OBJECT_IDS.iter().zip(sequences) {
            assert!(sequence.starts_with("\u{1b}_ratty;g;r;"));
            assert!(sequence.contains(&format!("id={id};")));
            assert!(sequence.contains("source=payload"));
        }
    }

    #[test]
    fn panel_placement_encodes_the_requested_terminal_window_rect() {
        let sequence = panel_placement_sequence(PANEL_OBJECT_IDS[2], Rect::new(8, 4, 32, 10));
        assert!(sequence.starts_with("\u{1b}_ratty;g;p;"));
        assert!(sequence.contains("row=8;col=23;w=32;h=10;"));
    }

    #[test]
    fn deleting_a_panel_uses_only_its_owned_object_id() {
        let sequence = panel_delete_sequence(PANEL_OBJECT_IDS[3]);
        assert_eq!(
            sequence,
            format!("\u{1b}_ratty;g;d;id={}\u{1b}\\", PANEL_OBJECT_IDS[3])
        );
        assert!(!sequence.contains(";d\u{1b}"));
    }

    #[test]
    fn widget_placement_preserves_the_cell_width_for_ratty_protocol_bytes() {
        let area = Rect::new(0, 0, 8, 1);
        let mut buffer = Buffer::empty(area);
        buffer.set_string(0, 0, "Menu    ", Style::default());
        assert_eq!(emit_panel_frames(&mut buffer, &[Rect::new(0, 0, 8, 1)]), 1);
        let cell = buffer.cell((0, 0)).unwrap();
        assert!(cell.symbol().starts_with("\u{1b}_ratty;g;p;"));
        assert_eq!(cell.cell_width(), 1);
    }

    #[test]
    fn layout_cleanup_only_deletes_frames_that_disappeared() {
        let mut bytes = Vec::new();
        write_panel_deletes(&mut bytes, 2, 5).unwrap();
        let output = String::from_utf8(bytes).unwrap();
        assert!(output.contains(&format!("id={}", PANEL_OBJECT_IDS[2])));
        assert!(output.contains(&format!("id={}", PANEL_OBJECT_IDS[4])));
        assert!(!output.contains(&format!("id={}", PANEL_OBJECT_IDS[1])));
    }
}

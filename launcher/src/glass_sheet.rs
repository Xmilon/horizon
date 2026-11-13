//! Instance settings as a sheet down the window's right side (the glass look): its icon, name and
//! what it plays at the top, then one section at a time, General, Installation, Launch and
//! Storage, picked on a segmented row. Home stays in view to its left, under the scrim. The
//! sections are the instance dialog's own parts (name, group, icon, versions, loader), the
//! memory row, the commands around the game and the export, gathered in one place.
use super::*;
use crate::glass_instance::section;
use crate::glass_ui::{self as ui, white};
use horizon_launcher::instances::{Instance, Kind as InstanceKind};
use horizon_theme::glass::{ink, radius};
use new_instance::NewKind;
use servers_page::Field;

/// The sheet's sections.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SheetTab {
    #[default]
    General,
    Installation,
    Launch,
    Storage,
}

/// How wide the sheet is, and its distance from the window's top (under the top bar) and edges.
const WIDTH: f32 = 660.;
const TOP: f32 = 88.;
const EDGE: f32 = 16.;

impl Launcher {
    /// The sheet, while the instance dialog edits an instance.
    pub(super) fn instance_sheet(&mut self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let t = self.theme;
        let d = self.instances.dialog.as_ref()?;
        let (busy, copy, error, kind) = (d.busy, d.copy, d.error.clone(), d.java.kind);
        let id = d.editing.clone()?;
        let editing = self.instances.list.iter().find(|i| i.id == id).cloned()?;
        // Launch and Storage act on the selected instance (`launch_page`, `export_page`).
        let selected = self.selected_instance().is_some_and(|s| s.id == editing.id);
        let tab = self.sheet_tab;

        let close = cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            if !busy {
                this.instances.dialog = None;
            }
            cx.notify();
        });
        let what = match kind {
            NewKind::Horizon => tr("Rust Edition"),
            NewKind::Vanilla => tr("Vanilla"),
            NewKind::Modded => tr("Modded"),
        };
        let icon = self.glass_instance_icon(&editing, 44.);
        let head = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(14.))
            .px(px(24.))
            .pt(px(22.))
            .pb(px(16.))
            .child(icon)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .font_family(horizon_theme::glass::DISPLAY_FONT)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(24.))
                            .line_height(px(30.))
                            .text_color(white(ink::FULL))
                            .child(editing.name.clone()),
                    )
                    .child(
                        ui::text(13., FontWeight::NORMAL, ink::NOTE)
                            .child(trf("Instance settings · {what}", &[("what", what)])),
                    ),
            )
            .child(
                ui::icon_button(
                    "sheet-close",
                    "cancel-01",
                    36.,
                    18.,
                    radius::CONTROL,
                    false,
                    &t,
                )
                .tooltip(ui::tip(tr("Close")))
                .when(!busy, |b| {
                    b.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.instances.dialog = None;
                        cx.notify();
                    }))
                }),
            );

        let this = cx.entity().downgrade();
        let tabs = div()
            .flex_none()
            .px(px(24.))
            .pb(px(14.))
            .child(ui::segments(
                "sheet-tabs",
                &[
                    (SheetTab::General, "General"),
                    (SheetTab::Installation, "Installation"),
                    (SheetTab::Launch, "Launch"),
                    (SheetTab::Storage, "Storage"),
                ],
                tab,
                &t,
                move |pick, _, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.sheet_tab = pick;
                        cx.notify();
                    });
                },
            ));

        let mut body = div()
            .id("sheet-body")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(28.))
            .px(px(24.))
            .pb(px(24.))
            .overflow_y_scroll();
        // The dialog's Save keeps name, group, icon and version: those two sections end with it.
        let mut foot = None;
        match tab {
            SheetTab::General => {
                let name = self
                    .field(Field::InstanceName, tr("My instance"), 300., cx)
                    .flex_1()
                    .min_w_0();
                let group = self
                    .field(Field::InstanceGroup, tr("No group"), 300., cx)
                    .flex_1()
                    .min_w_0();
                body = body
                    .child(section(
                        tr("Name and group").into(),
                        None,
                        div().flex().gap(px(10.)).child(name).child(group),
                    ))
                    .child(self.icon_choice(Some(&editing), kind, cx))
                    .child(self.sheet_actions(&editing, cx));
                foot = Some(self.instance_foot(Some(&editing), copy, busy, error, cx));
            }
            SheetTab::Installation => {
                body = body.children(self.switch_notice(&editing));
                if kind == NewKind::Horizon {
                    body = body.child(self.horizon_versions(cx));
                } else {
                    self.load_versions(cx);
                    body = body.child(self.minecraft_versions(cx));
                    if kind == NewKind::Modded {
                        body = body.child(self.loader_choice(cx));
                    }
                }
                foot = Some(self.instance_foot(Some(&editing), copy, busy, error, cx));
            }
            SheetTab::Launch if selected => {
                body = body
                    .children(self.glass_memory_row(cx))
                    .children(self.glass_hooks(cx));
            }
            SheetTab::Storage if selected => {
                if self.export.is_none() {
                    self.toggle_export();
                }
                body = body.children(self.export_panel(cx));
            }
            SheetTab::Launch | SheetTab::Storage => {
                body = body.child(ui::empty().child(tr(
                    "Pick this instance on Home to change how it launches and to export it.",
                )));
            }
        }

        let sheet = ui::dialog_plate(radius::MODAL)
            .id("instance-sheet")
            // Opaque under its glass: Home, not the sky, is behind it.
            .bg(t.side)
            .occlude()
            .absolute()
            .top(px(TOP))
            .right(px(EDGE))
            .bottom(px(EDGE))
            .w(px(WIDTH))
            .flex()
            .flex_col()
            .child(head)
            .child(tabs)
            .child(body)
            .children(foot.map(|f| f.px(px(24.)).py(px(16.))))
            .with_animation(
                ElementId::named_usize("sheet-in", 0),
                motion::eased(motion::POP),
                |d, p| d.opacity(p).right(px(EDGE - 24. * (1. - p))),
            );
        Some(
            div()
                .id("instance-sheet-layer")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_scroll(|_, _, cx| cx.stop_propagation())
                // A light dimming only: Home stays in view beside the sheet.
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .bg(ui::scrim().opacity(0.35))
                        .on_mouse_down(gpui::MouseButton::Left, close),
                )
                .child(sheet),
        )
    }

    /// General's quick actions: the folder, a desktop shortcut.
    fn sheet_actions(&mut self, editing: &Instance, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let home = editing.home.clone();
        let (id, name) = (editing.id.clone(), editing.name.clone());
        let shortcut = match self.has_shortcut(&id) {
            true => tr("Remove the desktop shortcut"),
            false => tr("Make a desktop shortcut"),
        };
        let mut row = div().flex().flex_wrap().gap(px(8.)).child(
            ui::quiet_icon(
                "sheet-folder",
                Some("folder-01"),
                tr("Open folder"),
                36.,
                &t,
            )
            .on_click(move |_, _, cx| {
                let _ = std::fs::create_dir_all(&home);
                crate::glass_toast::open(cx, &home);
            }),
        );
        if matches!(editing.kind, InstanceKind::Default | InstanceKind::Own) {
            row = row.child(
                ui::quiet_icon("sheet-shortcut", Some("link-01"), shortcut, 36., &t).on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.toggle_shortcut(id.clone(), name.clone());
                        cx.notify();
                    }),
                ),
            );
        }
        section(
            tr("Quick actions").into(),
            None,
            div().flex().flex_col().gap(px(10.)).child(row).child(
                ui::text(12.5, FontWeight::NORMAL, ink::FAINT)
                    .truncate()
                    .child(short(&editing.home)),
            ),
        )
    }
}

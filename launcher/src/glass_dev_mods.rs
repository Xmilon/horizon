//! The authors' place on Home's Mods tab: a mod added from its folder stays linked to it, so an
//! edit is one click (Build and play) from the game. Out of players' way: only a quiet "Add a mod
//! folder" link until a folder mod is added, then a section with the folders, the last build's
//! time or first error, and how the game runs.
use super::*;
use crate::glass_ui as ui;
use horizon_launcher::folder_mods;
use horizon_theme::glass::ink;
use std::path::PathBuf;

impl Launcher {
    /// The selected instance's folder mods, read once per instance and after a change.
    fn dev_mods(&self) -> Vec<(String, PathBuf)> {
        let home = self.mods.home();
        let mut memo = self.dev_mods.borrow_mut();
        match memo.as_ref() {
            Some((h, found)) if *h == home => found.clone(),
            _ => {
                let found = folder_mods::all(&home);
                *memo = Some((home, found.clone()));
                found
            }
        }
    }

    /// Asks for a folder of mod projects and links them into the selected instance.
    fn dev_add_folder(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("Add mod folder").into()),
        });
        let home = self.mods.home();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(mut picked))) = paths.await else {
                return;
            };
            let Some(root) = picked.pop() else { return };
            let added = cx
                .background_spawn(async move { folder_mods::add(&home, &root) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.mods.message = Some(match added {
                    Ok(ids) => Ok(trf("Added {mods} from its folder.", &[("mods", &ids.join(", "))])),
                    Err(e) => Err(e),
                });
                *this.dev_mods.borrow_mut() = None;
                this.instance_mods_changed();
                cx.notify();
            });
        })
        .detach();
    }

    /// Packs the folder mods again from their folders, then builds and plays.
    fn dev_build_and_play(&mut self) {
        if let Err(e) = folder_mods::refresh(&self.mods.home()) {
            self.mods.message = Some(Err(e));
            return;
        }
        self.instance_mods_changed();
        self.play_after_build = true;
        if self.settings.native_mods_allowed_at.is_some() {
            self.build_native();
        } else {
            self.request_native_build_consent();
        }
    }

    /// The quiet entry for authors, under the list.
    pub(super) fn dev_entry(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.mods.java.is_some() || !self.dev_mods().is_empty() {
            return None;
        }
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .pt(px(12.))
                .child(
                    ui::text(13., FontWeight::NORMAL, ink::NOTE)
                        .child(tr("Making a mod?")),
                )
                .child(
                    ui::link("dev-add-folder", tr("Add its folder")).on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            this.dev_add_folder(cx);
                            cx.notify();
                        },
                    )),
                ),
        )
    }

    /// The authors' section, once a folder mod is in the instance.
    pub(super) fn dev_panel(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.mods.java.is_some() {
            return None;
        }
        let linked = self.dev_mods();
        if linked.is_empty() {
            return None;
        }
        let t = self.theme;
        let instance = self.selected_instance()?;
        let mine = self.native_for.as_deref() == Some(instance.id.as_str());
        let tag = self.version_of(instance).cloned().unwrap_or_default();
        let game = if horizon_launcher::developer_offline::enabled(&tag) {
            tr("Developer build, offline single-player").to_string()
        } else if horizon_launcher::developer_install::is_developer(&tag) {
            crate::glass_instance::version_label(&tag)
        } else if tag.is_empty() {
            tr("Latest version").to_string()
        } else {
            crate::glass_instance::version_label(&tag)
        };
        // The last build: its time when it worked, else its first error with file and line.
        let last = if let Some(failed) = self.native_failed.as_ref().filter(|_| mine) {
            failed
                .lines
                .first()
                .cloned()
                .map(|line| trf("First error: {line}", &[("line", &line)]))
        } else {
            self.native_done.clone().filter(|_| mine)
        };
        let fact = |label: &str, value: String| {
            div()
                .flex()
                .gap(px(12.))
                .child(
                    ui::text(13., FontWeight::NORMAL, ink::NOTE)
                        .w(px(110.))
                        .flex_none()
                        .child(tr(label).to_string()),
                )
                .child(
                    ui::text(13., FontWeight::NORMAL, ink::BODY)
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(value),
                )
        };
        let mut facts = div().flex().flex_col().gap(px(6.));
        for (id, folder) in &linked {
            let open = folder.clone();
            let unlink = id.clone();
            facts = facts.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        fact(id, crate::glass_toast::tilde(folder)).flex_1().min_w_0(),
                    )
                    .child(
                        ui::link(SharedString::from(format!("dev-open-{id}")), tr("Open"))
                            .on_click(move |_, _, cx| crate::glass_toast::open(cx, &open)),
                    )
                    .child(
                        ui::link(SharedString::from(format!("dev-unlink-{id}")), tr("Remove"))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if let Err(e) = folder_mods::remove(&this.mods.home(), &unlink) {
                                    this.mods.message = Some(Err(e));
                                }
                                *this.dev_mods.borrow_mut() = None;
                                this.instance_mods_changed();
                                cx.notify();
                            })),
                    ),
            );
        }
        facts = facts
            .child(fact(
                "Last build",
                last.unwrap_or_else(|| tr("Not built yet").to_string()),
            ))
            .child(fact("Game", game));
        let building = mine && self.native_step.is_some();
        let play = button("dev-build-play", "Build and play", Kind::Primary, &t);
        let play = if building || self.native_step.is_some() {
            play.opacity(0.5)
        } else {
            play.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.dev_build_and_play();
                cx.notify();
            }))
        };
        Some(
            crate::glass::plate(horizon_theme::glass::radius::PANEL)
                .flex()
                .flex_col()
                .gap(px(12.))
                .p(px(16.))
                .mb(px(12.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(ui::icon("computer-terminal-01", 16., ui::white(ink::BODY)))
                        .child(
                            ui::text(14., FontWeight::SEMIBOLD, ink::FULL)
                                .child(tr("Developing")),
                        ),
                )
                .child(facts)
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .child(play)
                        .child(
                            button("dev-add-more", "Add a mod folder", Kind::Secondary, &t)
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.dev_add_folder(cx);
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    ui::text(12., FontWeight::NORMAL, ink::NOTE).child(tr(
                        "Build and play packs each folder again, so your latest edits are in the game.",
                    )),
                ),
        )
    }
}

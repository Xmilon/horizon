//! What the launcher says when something stands between the player and their game: a mod that
//! didn't build, a game update some mods don't fit, worlds saved with a mod a build leaves out.
//! Each names the mod, leads with the way back to playing, and keeps the details one click away.
use super::*;
use std::path::Path;

/// How many of the build log's lines a failed build shows; the whole log is one click away.
const KEY_LINES: usize = 3;

/// A build stopped by mods that didn't build or fit: which ones, and the log lines that say why.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FailedBuild {
    /// The mods that stopped it, by id.
    pub(crate) mods: Vec<String>,
    /// The native mods turned on beside them, which are fine.
    pub(crate) others: usize,
    /// The log's key lines: its last errors.
    pub(crate) lines: Vec<String>,
}

impl FailedBuild {
    /// Read once, when the build ends: the mods turned on in `home` and the build log.
    pub(crate) fn new(mods: Vec<String>, home: &Path) -> Self {
        let others = native_status::enabled(home)
            .iter()
            .filter(|(id, ..)| !mods.contains(id))
            .count();
        let lines = std::fs::read_to_string(native_mods::log_path())
            .map(|log| key_lines(&log))
            .unwrap_or_default();
        Self {
            mods,
            others,
            lines,
        }
    }
}

/// The last [`KEY_LINES`] lines of `log` that say error, else its last lines.
fn key_lines(log: &str) -> Vec<String> {
    let lines: Vec<&str> = log
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    let errors: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| l.to_ascii_lowercase().contains("error"))
        .collect();
    let pick = if errors.is_empty() { &lines } else { &errors };
    pick[pick.len().saturating_sub(KEY_LINES)..]
        .iter()
        .map(|l| l.to_string())
        .collect()
}

/// The turned-on native mod a crash's `text` points at, when exactly one does: Rust names a
/// mod's code by its crate (`my_mod::`) in panics and backtraces.
pub(crate) fn crash_suspect(home: &Path, text: &str) -> Option<String> {
    let ids: Vec<String> = native_status::enabled(home)
        .into_iter()
        .map(|(id, ..)| id)
        .collect();
    suspect_in(&ids, text)
}

fn suspect_in(ids: &[String], text: &str) -> Option<String> {
    let named: Vec<&String> = ids
        .iter()
        .filter(|id| text.contains(&format!("{}::", id.replace('-', "_"))))
        .collect();
    match named.as_slice() {
        [one] => Some((*one).clone()),
        _ => None,
    }
}

/// "a", "a and b", "a, b and c".
fn names(mods: &[String]) -> String {
    match mods {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => trf(
            "{list} and {last}",
            &[("list", &rest.join(", ")), ("last", last)],
        ),
    }
}

/// The log lines as a terminal shows them.
fn terminal(lines: &[String], pal: Pal, t: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .p(px(12.))
        .rounded(px(10.))
        .when(t.glass, |d| d.bg(gpui::black().opacity(0.42)))
        .when(!t.glass, |d| d.bg(t.well))
        .font_family(theme::MONO)
        .text_size(px(12.))
        .line_height(px(18.))
        .children(lines.iter().map(|line| {
            div()
                .truncate()
                .text_color(if line.to_ascii_lowercase().contains("error") {
                    pal.danger
                } else {
                    pal.muted
                })
                .child(line.clone())
        }))
}

impl Launcher {
    /// Sample states for screenshots (`HORIZON_LAUNCHER_PAGE=guard-…`, debug builds only).
    pub(crate) fn guard_preview(&mut self, page: &str) {
        let here = self.selected_instance().map(|i| i.id.clone());
        match page {
            "guard-crash" => {
                self.crash = Some(Crash {
                    report: None,
                    summary: "thread 'main' panicked at fast_ore::tick: index out of bounds".into(),
                    copied: false,
                    suspect: Some("fast-ore".into()),
                    game: "nightly-2026-11-11-4baa571".into(),
                    mods: vec!["fast-ore 0.3.1".into(), "greeter 1.0.0".into()],
                    text: String::new(),
                })
            }
            "guard-worlds" => {
                self.native_for = here;
                self.native_worlds = Some(native_mods::AtStake {
                    text: String::new(),
                    worlds: vec!["Survival with Sam".into(), "Creative build".into()],
                    mods: vec!["fast-ore".into()],
                    unbuilt: Vec::new(),
                    backups: backend::data_dir().join("backups"),
                    stay: "Nov 11 update".into(),
                    base: "nightly-2026-11-13-c9d474d".into(),
                });
                self.native_error = Some(String::new());
            }
            "guard-failed" => {
                self.native_for = here;
                self.native_error = Some(String::new());
                self.native_failed = Some(FailedBuild {
                    mods: vec!["fast-ore".into()],
                    others: 2,
                    lines: vec![
                        "error[E0425]: cannot find value `ORE_TICKS` in this scope".into(),
                        "  --> fast-ore/src/lib.rs:42:9".into(),
                        "error: could not compile `fast_ore` (lib) due to 1 previous error".into(),
                    ],
                });
            }
            _ => {}
        }
    }

    /// Turns `ids` off in the selected instance, then builds the rest and plays: a failed build's
    /// and a crash's way back into the game.
    pub(crate) fn turn_off_and_play(&mut self, ids: &[String]) {
        let home = self.mods.home();
        let Some(lock) = horizon_launcher::browser::try_lock_home(&home) else {
            self.mods.message = Some(Err(tr("Wait for the current operation to finish.").into()));
            return;
        };
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let result = horizon_launcher::browser::java::set_all_enabled(&home, &refs, false);
        drop(lock);
        if let Err(e) = result {
            self.mods.message = Some(Err(e));
            return;
        }
        self.native_failed = None;
        self.native_error = None;
        self.instance_mods_changed();
        if native_status::enabled(&home).is_empty() {
            self.play();
        } else {
            self.play_after_build = true;
            self.build_native();
        }
    }

    /// Home's copy of the failed build's card, above the instance's tabs: the build stands
    /// between the player and Play.
    pub(crate) fn home_failed_build(&self, cx: &mut Context<Self>) -> Option<Div> {
        let here = self.selected_instance()?.id.as_str();
        let failed = self.native_failed.as_ref().filter(|_| {
            self.native_for.as_deref() == Some(here)
                && self.native_error.is_some()
                && self.native_step.is_none()
        })?;
        // On the crash card's plate: the page's sky would show through the Mods tab's panel.
        let card = self
            .failed_card(failed, Pal::of(&self.theme), cx)
            .bg(gpui::transparent_black())
            .p(px(24.));
        Some(plate(&self.theme).child(card))
    }

    /// A build that stopped on mods: which, that the others are fine, the way back to playing,
    /// the log's key lines, the whole log and a report for the mod's author.
    pub(super) fn failed_card(
        &self,
        failed: &FailedBuild,
        pal: Pal,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.theme;
        let one = failed.mods.len() == 1;
        let title = if one {
            trf("{mod} didn't build", &[("mod", &failed.mods[0])])
        } else {
            trf(
                "{count} mods didn't build: {mods}",
                &[
                    ("count", &failed.mods.len().to_string()),
                    ("mods", &names(&failed.mods)),
                ],
            )
        };
        let note = match (failed.others, one) {
            (0, _) => tr("Turning it off lets the game start without it."),
            (_, true) => tr("Your other mods are fine: only this one stopped the build."),
            (_, false) => tr("Your other mods are fine: only these stopped the build."),
        };
        let ids = failed.mods.clone();
        let off = button(
            "failed-off",
            if one {
                "Turn it off and play"
            } else {
                "Turn them off and play"
            },
            Kind::Primary,
            &t,
        )
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.turn_off_and_play(&ids);
            cx.notify();
        }));
        let log = button("failed-log", "Full log", Kind::Secondary, &t).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.native_log_open();
                cx.notify();
            },
        ));
        let home = self.mods.home();
        let report_for = failed.clone();
        let report = button("failed-report", "Report to the author", Kind::Ghost, &t).on_click(
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.report_failed(&report_for, &home, cx);
                cx.notify();
            }),
        );
        let head = div()
            .flex()
            .items_start()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(36.))
                    .rounded(px(10.))
                    .bg(pal.danger.opacity(0.14))
                    .child(ui::icon("alert-02", 18., pal.danger)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        text_in(15., pal.ink)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(text_in(13., pal.muted).child(note.to_string())),
            );
        self.panel()
            .child(head)
            .when(!failed.lines.is_empty(), |d| {
                d.child(terminal(&failed.lines, pal, &t))
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(off)
                    .child(log)
                    .child(report),
            )
    }

    /// Copies a report for the mod's author (the mods, the game build, the whole build log) and
    /// opens where the author takes them, when the mod came from GitHub.
    fn report_failed(&mut self, failed: &FailedBuild, home: &Path, cx: &mut Context<Self>) {
        let game = self
            .selected_instance()
            .and_then(|i| self.version_of(i))
            .cloned()
            .unwrap_or_default();
        let log = std::fs::read_to_string(native_mods::log_path()).unwrap_or_default();
        let versions: Vec<String> = native_status::enabled(home)
            .into_iter()
            .filter(|(id, ..)| failed.mods.contains(id))
            .map(|(id, version, _)| format!("{id} {version}"))
            .collect();
        let text = format!(
            "Didn't build: {}\nGame: {game}\n\n{log}",
            versions.join(", ")
        );
        copy_to_clipboard(cx, gpui::ClipboardItem::new_string(text));
        let origin = failed
            .mods
            .first()
            .and_then(|id| horizon_launcher::github_mods::origin(home, id));
        match origin {
            Some(o) => {
                cx.open_url(&format!(
                    "https://github.com/{}/{}/issues/new",
                    o.owner, o.repo
                ));
                crate::glass_toast::say(tr("Report copied: paste it into the new issue."));
            }
            None => crate::glass_toast::say(tr(
                "Report copied: paste it where the mod's author takes reports.",
            )),
        }
    }

    /// Before an update: how many mods fit the new game first, then only those that don't, each
    /// with its reason behind Why?. Staying is a choice as plain as updating; when worlds were
    /// saved with a mod it turns off, staying is the default.
    pub(super) fn update_panel(
        &self,
        check: native_mods::UpdateCheck,
        plays: &str,
        key: String,
        pal: Pal,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.theme;
        let current = native_mods::about(plays).map_or_else(|| plays.to_owned(), |a| a.base);
        let fit = check.total.saturating_sub(check.off.len());
        let head = match (check.off.is_empty(), check.total) {
            (true, 1) => trf("Your mod fits {build}", &[("build", &check.build)]),
            (true, _) => trf(
                "All {count} mods fit {build}",
                &[("count", &check.total.to_string()), ("build", &check.build)],
            ),
            (false, _) => trf(
                "{fit} of {count} mods fit {build}",
                &[
                    ("fit", &fit.to_string()),
                    ("count", &check.total.to_string()),
                    ("build", &check.build),
                ],
            ),
        };
        let back_up = !check.worlds.is_empty();
        let rows = check.off.iter().map(|(id, why)| {
            let open = self.update_why.as_deref() == Some(id.as_str());
            let toggle = id.clone();
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .py(px(8.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            text_in(13., pal.ink)
                                .font_weight(FontWeight::MEDIUM)
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(id.clone()),
                        )
                        .child(text_in(12., pal.warn).child(tr("Off until its author updates it")))
                        .child(
                            text_in(12., pal.muted)
                                .id(SharedString::from(format!("update-why-{id}")))
                                .cursor_pointer()
                                .hover(|s| s.underline())
                                .child(tr("Why?"))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.update_why = if this.update_why.as_deref()
                                        == Some(toggle.as_str())
                                    {
                                        None
                                    } else {
                                        Some(toggle.clone())
                                    };
                                    cx.notify();
                                })),
                        ),
                )
                .when(open && !why.is_empty(), |d| {
                    d.child(text_in(12., pal.muted).child(why.clone()))
                })
        });
        let stay = button(
            "native-update-stay",
            native_mods::fit::stay(&current),
            if back_up {
                Kind::Primary
            } else {
                Kind::Secondary
            },
            &t,
        )
        .flex_1()
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            *this.mods.update_stayed.borrow_mut() = Some(key.clone());
            cx.notify();
        }));
        let to = check.to.clone();
        let take = button(
            "native-update-take",
            if back_up {
                native_mods::fit::back_up_and_update(check.worlds.len())
            } else {
                native_mods::fit::update()
            },
            Kind::Secondary,
            &t,
        )
        .flex_1()
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.build_native_on(Some(to.clone()), back_up);
            cx.notify();
        }));
        self.panel()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(ui::icon("refresh", 16., pal.accent))
                    .child(text_in(13., pal.muted).child(tr("A game update is ready"))),
            )
            .child(
                text_in(18., pal.ink)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(head),
            )
            .when(!check.off.is_empty(), |d| {
                d.child(div().flex().flex_col().children(rows))
            })
            .when(back_up, |d| {
                d.child(
                    text_in(13., pal.warn).child(native_mods::fit::update_worlds(&check.worlds)),
                )
            })
            // Equal choices, side by side; the safe one first when worlds are at stake.
            .child(if back_up {
                div().flex().gap(px(8.)).child(stay).child(take)
            } else {
                div().flex().gap(px(8.)).child(take).child(stay)
            })
    }

    /// Worlds saved with mods a build would leave out: a dialog of its own, amber, naming the
    /// worlds and the mods. Not building is the primary choice; the other backs every world
    /// listed up first, then builds without the mods.
    pub(crate) fn worlds_dialog(&self, cx: &mut Context<Self>) -> Option<Div> {
        let stake = self.native_worlds.as_ref().filter(|_| self.native_step.is_none())?;
        let t = self.theme;
        let pal = Pal::of(&t);
        let mods = names(&stake.mods);
        let title = match stake.worlds.as_slice() {
            [world] => trf(
                "“{world}” was saved with {mods}",
                &[("world", world), ("mods", &mods)],
            ),
            worlds => trf(
                "{count} worlds were saved with {mods}",
                &[("count", &worlds.len().to_string()), ("mods", &mods)],
            ),
        };
        let body = if stake.mods.len() == 1 {
            trf(
                "This build can't include {mods}. Opening a world without it removes its blocks and what they hold, for good.",
                &[("mods", &mods)],
            )
        } else {
            trf(
                "This build can't include {mods}. Opening a world without them removes their blocks and what they hold, for good.",
                &[("mods", &mods)],
            )
        };
        let worlds = stake.worlds.iter().map(|world| {
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .py(px(8.))
                .px(px(12.))
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(px(18.))
                        .rounded(px(5.))
                        .bg(pal.warn)
                        .child(ui::icon("tick-02", 13., gpui::black())),
                )
                .child(
                    text_in(13., pal.ink)
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(world.clone()),
                )
                .child(text_in(12., pal.muted).child(tr("Backed up first")))
        });
        let base = stake.base.clone();
        let back_up = button(
            "native-worlds-back-up",
            native_mods::fit::back_up_and_build(
                stake.worlds.len(),
                stake.mods.len(),
                stake.unbuilt.len(),
            ),
            Kind::Secondary,
            &t,
        )
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.build_native_on(Some(base.clone()), true);
            cx.notify();
        }));
        let stay = button(
            "native-worlds-stay",
            native_mods::fit::dont_build().to_string(),
            Kind::Primary,
            &t,
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.native_worlds = None;
            this.native_error = None;
            this.play_after_build = false;
            cx.notify();
        }));
        let card = if t.glass {
            ui::dialog_plate(horizon_theme::glass::radius::MODAL)
        } else {
            plate(&t).bg(t.surface).rounded(px(16.))
        };
        let card = card
            .relative()
            .occlude()
            .w(px(540.))
            .p(px(28.))
            .border_1()
            .border_color(pal.warn.opacity(0.45))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(44.))
                    .rounded(px(12.))
                    .bg(pal.warn.opacity(0.16))
                    .child(ui::icon("alert-02", 22., pal.warn)),
            )
            .child(
                text_in(20., pal.ink)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(text_in(14., pal.ink).child(body))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(px(10.))
                    .bg(pal.warn.opacity(0.07))
                    .children(worlds),
            )
            .child(text_in(12., pal.muted).child(trf(
                "Backups go to {path}, one zip per world.",
                &[("path", &stake.backups.display().to_string())],
            )))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .flex_wrap()
                    .gap(px(10.))
                    .child(back_up)
                    .child(stay),
            );
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .bg(crate::glass_ui::scrim())
                .child(card),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_lines_prefer_the_last_errors() {
        let log = "Compiling a\nerror[E0425]: x\nwarning: y\nerror: could not compile `greeter`\n\nDone\n";
        assert_eq!(
            key_lines(log),
            ["error[E0425]: x", "error: could not compile `greeter`"]
        );
        assert_eq!(key_lines("one\ntwo\nthree\nfour"), ["two", "three", "four"]);
    }

    #[test]
    fn a_crash_names_a_mod_only_when_one_crate_matches() {
        let ids = ["greeter".to_string(), "fast-ore".to_string()];
        assert_eq!(
            suspect_in(&ids, "panicked at fast_ore::tick: boom"),
            Some("fast-ore".to_string())
        );
        assert_eq!(suspect_in(&ids, "greeter::a then fast_ore::b"), None);
        assert_eq!(suspect_in(&ids, "nothing here"), None);
    }
}

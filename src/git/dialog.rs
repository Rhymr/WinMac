use super::ops::{GitController, GitFileStatus};
use crate::workspace::controller::WorkspaceController;
use futures_util::StreamExt;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Label, ListBox, Orientation, ScrolledWindow, Spinner, TextBuffer,
    TextView, Window,
};
use libadwaita::Application;
use std::path::PathBuf;
use std::rc::Rc;

const REMOTE: &str = "origin";

pub fn show_commit_dialog(app: &Application, controller: Rc<WorkspaceController>) {
    let Some(parent) = app.active_window() else {
        return;
    };
    let Some(root) = controller.get_root_path() else {
        return;
    };

    let git = GitController::new(&root);
    let mut changed: Vec<(PathBuf, GitFileStatus)> = git.file_statuses().into_iter().collect();
    changed.sort_by(|a, b| a.0.cmp(&b.0));

    let dialog = Window::builder()
        .title("Commit Changes")
        .transient_for(&parent)
        .modal(true)
        .default_width(480)
        .default_height(420)
        .build();

    let content = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();

    content.append(
        &Label::builder()
            .label(format!("{} changed file(s)", changed.len()))
            .halign(Align::Start)
            .css_classes(vec!["field-label"])
            .build(),
    );

    let files_list = ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(vec!["commit-file-list"])
        .build();
    if changed.is_empty() {
        files_list.append(
            &Label::builder()
                .label("No changes to commit.")
                .halign(Align::Start)
                .css_classes(vec!["dim-label"])
                .build(),
        );
    } else {
        for (path, status) in &changed {
            let marker = match status {
                GitFileStatus::New => "A",
                GitFileStatus::Renamed => "R",
                GitFileStatus::Modified => "M",
            };
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .display()
                .to_string();
            files_list.append(
                &Label::builder()
                    .label(format!("[{marker}]  {rel}"))
                    .halign(Align::Start)
                    .build(),
            );
        }
    }
    let files_scroll = ScrolledWindow::builder()
        .vexpand(true)
        .child(&files_list)
        .css_classes(vec!["commit-file-scroll"])
        .build();
    content.append(&files_scroll);

    content.append(
        &Label::builder()
            .label("Commit message:")
            .halign(Align::Start)
            .css_classes(vec!["field-label"])
            .margin_top(8)
            .build(),
    );

    let message_buffer = TextBuffer::builder().build();
    let message_view = TextView::builder()
        .buffer(&message_buffer)
        .wrap_mode(gtk::WrapMode::Word)
        .build();
    let message_scroll = ScrolledWindow::builder()
        .height_request(90)
        .child(&message_view)
        .css_classes(vec!["commit-message-box"])
        .build();
    content.append(&message_scroll);

    let action_box = GtkBox::new(Orientation::Horizontal, 10);
    action_box.set_halign(Align::End);
    action_box.set_margin_top(4);
    let cancel_btn = Button::builder().label("Cancel").build();
    let commit_btn = Button::builder()
        .label("Commit")
        .css_classes(vec!["suggested-action"])
        .sensitive(!changed.is_empty())
        .build();
    action_box.append(&cancel_btn);
    action_box.append(&commit_btn);
    content.append(&action_box);

    dialog.set_child(Some(&content));

    let dialog_for_cancel = dialog.clone();
    cancel_btn.connect_clicked(move |_| dialog_for_cancel.close());

    let dialog_for_commit = dialog.clone();
    commit_btn.connect_clicked(move |_| {
        let text = message_buffer
            .text(
                &message_buffer.start_iter(),
                &message_buffer.end_iter(),
                false,
            )
            .to_string();
        let message = if text.trim().is_empty() {
            "Update".to_string()
        } else {
            text.trim().to_string()
        };

        match GitController::new(&root).commit_all(&message) {
            Ok(_) => {
                refresh_file_tree(&controller);
                dialog_for_commit.close();
            }
            Err(e) => eprintln!("Commit failed: {e}"),
        }
    });

    dialog.present();
}

pub fn show_push_dialog(app: &Application, controller: Rc<WorkspaceController>) {
    confirm_then_run(
        app,
        controller,
        "Push",
        |branch| format!("Push '{branch}' to {REMOTE}?"),
        "Push",
        |git| git.push(REMOTE),
    );
}

pub fn show_pull_dialog(app: &Application, controller: Rc<WorkspaceController>) {
    confirm_then_run(
        app,
        controller,
        "Update Project",
        |branch| {
            format!(
                "Pull '{branch}' from {REMOTE}? This fast-forwards only — it won't merge or rebase if the branches have diverged."
            )
        },
        "Pull",
        |git| git.pull(REMOTE),
    );
}

pub fn show_fetch_dialog(app: &Application, controller: Rc<WorkspaceController>) {
    let Some(parent) = app.active_window() else {
        return;
    };
    let Some(root) = controller.get_root_path() else {
        return;
    };

    let (dialog, content, status_label) =
        build_progress_dialog(&parent, "Fetch", &format!("Fetching from {REMOTE}…"));
    dialog.present();
    run_git_op(dialog, content, status_label, root, controller, |git| {
        git.fetch(REMOTE)
    });
}

/// Shows a Cancel/[`action_label`] confirmation with `confirm_text(branch)`
/// as its message; on confirming, swaps the same window's content for a
/// spinner, runs `op` on a background thread (network git ops shouldn't
/// block the GTK main loop), and reports the result.
fn confirm_then_run(
    app: &Application,
    controller: Rc<WorkspaceController>,
    title: &str,
    confirm_text: impl Fn(&str) -> String + 'static,
    action_label: &str,
    op: impl Fn(&GitController) -> Result<String, String> + Clone + Send + 'static,
) {
    let Some(parent) = app.active_window() else {
        return;
    };
    let Some(root) = controller.get_root_path() else {
        return;
    };

    let git = GitController::new(&root);
    if !git.has_remote(REMOTE) {
        show_message_dialog(
            &parent,
            title,
            &format!("No '{REMOTE}' remote is configured for this project."),
        );
        return;
    }
    let branch = git
        .current_branch_name()
        .unwrap_or_else(|| "HEAD".to_string());

    let dialog = Window::builder()
        .title(title)
        .transient_for(&parent)
        .modal(true)
        .default_width(420)
        .resizable(false)
        .build();

    let content = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(16)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let message_label = Label::builder()
        .label(confirm_text(&branch))
        .halign(Align::Start)
        .wrap(true)
        .build();
    content.append(&message_label);

    let action_box = GtkBox::new(Orientation::Horizontal, 10);
    action_box.set_halign(Align::End);
    let cancel_btn = Button::builder().label("Cancel").build();
    let confirm_btn = Button::builder()
        .label(action_label)
        .css_classes(vec!["suggested-action"])
        .build();
    action_box.append(&cancel_btn);
    action_box.append(&confirm_btn);
    content.append(&action_box);

    dialog.set_child(Some(&content));

    let dialog_for_cancel = dialog.clone();
    cancel_btn.connect_clicked(move |_| dialog_for_cancel.close());

    let dialog_for_confirm = dialog.clone();
    let content_for_confirm = content.clone();
    let message_label_for_confirm = message_label.clone();
    let action_box_for_confirm = action_box.clone();
    let root_for_confirm = root.clone();
    let controller_for_confirm = controller.clone();
    confirm_btn.connect_clicked(move |_| {
        content_for_confirm.remove(&action_box_for_confirm);
        message_label_for_confirm.set_text("Working…");
        let spinner = Spinner::builder()
            .spinning(true)
            .width_request(20)
            .height_request(20)
            .halign(Align::Center)
            .build();
        content_for_confirm.append(&spinner);

        run_git_op(
            dialog_for_confirm.clone(),
            content_for_confirm.clone(),
            message_label_for_confirm.clone(),
            root_for_confirm.clone(),
            controller_for_confirm.clone(),
            op.clone(),
        );
    });

    dialog.present();
}

/// Builds a title + spinner + status-message dialog, already showing
/// `initial_text` — used directly by `show_fetch_dialog` (which has no
/// confirmation step) and indirectly by `confirm_then_run`, which builds
/// its own confirm step first and only reaches this shape once confirmed.
fn build_progress_dialog(
    parent: &gtk::Window,
    title: &str,
    initial_text: &str,
) -> (Window, GtkBox, Label) {
    let dialog = Window::builder()
        .title(title)
        .transient_for(parent)
        .modal(true)
        .default_width(420)
        .resizable(false)
        .build();

    let content = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(16)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let status_label = Label::builder()
        .label(initial_text)
        .halign(Align::Start)
        .wrap(true)
        .build();
    content.append(&status_label);
    let spinner = Spinner::builder()
        .spinning(true)
        .width_request(20)
        .height_request(20)
        .halign(Align::Center)
        .build();
    content.append(&spinner);

    dialog.set_child(Some(&content));
    (dialog, content, status_label)
}

/// Runs `op` on a background thread and reports the result into
/// `status_label`, replacing whatever spinner/progress widget is currently
/// the last child of `content` with a Close button.
fn run_git_op(
    dialog: Window,
    content: GtkBox,
    status_label: Label,
    root: PathBuf,
    controller: Rc<WorkspaceController>,
    op: impl FnOnce(&GitController) -> Result<String, String> + Send + 'static,
) {
    let (sender, mut receiver) = futures_channel::mpsc::unbounded::<Result<String, String>>();
    std::thread::spawn(move || {
        let git = GitController::new(&root);
        let _ = sender.unbounded_send(op(&git));
    });

    glib::MainContext::default().spawn_local(async move {
        let Some(result) = receiver.next().await else {
            return;
        };

        if let Some(spinner) = content.last_child() {
            content.remove(&spinner);
        }

        match result {
            Ok(msg) => status_label.set_text(&msg),
            Err(e) => status_label.set_text(&format!("Failed: {e}")),
        }

        let close_box = GtkBox::new(Orientation::Horizontal, 0);
        close_box.set_halign(Align::End);
        let close_btn = Button::builder().label("Close").build();
        let dialog_for_close = dialog.clone();
        close_btn.connect_clicked(move |_| dialog_for_close.close());
        close_box.append(&close_btn);
        content.append(&close_box);

        refresh_file_tree(&controller);
    });
}

fn show_message_dialog(parent: &gtk::Window, title: &str, message: &str) {
    let dialog = Window::builder()
        .title(title)
        .transient_for(parent)
        .modal(true)
        .default_width(380)
        .resizable(false)
        .build();

    let content = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(16)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(
        &Label::builder()
            .label(message)
            .halign(Align::Start)
            .wrap(true)
            .build(),
    );

    let close_box = GtkBox::new(Orientation::Horizontal, 0);
    close_box.set_halign(Align::End);
    let close_btn = Button::builder().label("Close").build();
    let dialog_for_close = dialog.clone();
    close_btn.connect_clicked(move |_| dialog_for_close.close());
    close_box.append(&close_btn);
    content.append(&close_box);

    dialog.set_child(Some(&content));
    dialog.present();
}

fn refresh_file_tree(controller: &Rc<WorkspaceController>) {
    if let Some(workspace) = controller.get_workspace()
        && let Some(ref file_tree) = workspace.file_tree
    {
        file_tree.refresh();
    }
}

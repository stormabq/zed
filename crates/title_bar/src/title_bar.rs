mod application_menu;
mod collab;
mod platforms;
mod window_controls;

#[cfg(feature = "stories")]
mod stories;

use crate::application_menu::ApplicationMenu;
use crate::platforms::{platform_linux, platform_mac, platform_windows};
use auto_update::AutoUpdateStatus;
use call::ActiveCall;
use client::{Client, UserStore};
use feature_flags::{FeatureFlagAppExt, ZedPro};
use gpui::{
    actions, div, px, Action, AnyElement, AppContext, AppContext, Decorations, Element,
    InteractiveElement, Interactivity, IntoElement, Model, MouseButton, ParentElement, Render,
    Stateful, StatefulInteractiveElement, Styled, Subscription, View, VisualContext, WeakView,
};
use project::{Project, RepositoryEntry};
use rpc::proto;
use settings::Settings as _;
use smallvec::SmallVec;
use std::sync::Arc;
use theme::ActiveTheme;
use ui::{
    h_flex, prelude::*, Avatar, Button, ButtonLike, ButtonStyle, ContextMenu, Icon, IconName,
    IconSize, IconWithIndicator, Indicator, PopoverMenu, Tooltip,
};
use util::ResultExt;
use workspace::{notifications::NotifyResultExt, Workspace};
use zed_actions::{OpenBrowser, OpenRecent, OpenRemote};

#[cfg(feature = "stories")]
pub use stories::*;

const MAX_PROJECT_NAME_LENGTH: usize = 40;
const MAX_BRANCH_NAME_LENGTH: usize = 40;

const BOOK_ONBOARDING: &str = "https://dub.sh/zed-onboarding";

actions!(
    collab,
    [
        ShareProject,
        UnshareProject,
        ToggleUserMenu,
        ToggleProjectMenu,
        SwitchBranch
    ]
);

pub fn init(cx: &mut AppContext) {
    cx.observe_new_views(|workspace: &mut Workspace, cx| {
        let item = cx.new_model(|model, cx| TitleBar::new("title-bar", workspace, model, cx));
        workspace.set_titlebar_item(item.into(), model, cx)
    })
    .detach();
}

pub struct TitleBar {
    platform_style: PlatformStyle,
    content: Stateful<Div>,
    children: SmallVec<[AnyElement; 2]>,
    project: Model<Project>,
    user_store: Model<UserStore>,
    client: Arc<Client>,
    workspace: WeakModel<Workspace>,
    should_move: bool,
    application_menu: Option<Model<ApplicationMenu>>,
    _subscriptions: Vec<Subscription>,
}

impl Render for TitleBar {
    fn render(
        &mut self,
        model: &Model<Self>,
        window: &mut gpui::Window,
        cx: &mut AppContext,
    ) -> impl IntoElement {
        let close_action = Box::new(workspace::CloseWindow);
        let height = Self::height(model, cx);
        let supported_controls = cx.window_controls();
        let decorations = cx.window_decorations();
        let titlebar_color = if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            if cx.is_window_active() && !self.should_move {
                cx.theme().colors().title_bar_background
            } else {
                cx.theme().colors().title_bar_inactive_background
            }
        } else {
            cx.theme().colors().title_bar_background
        };

        h_flex()
            .id("titlebar")
            .w_full()
            .h(height)
            .map(|this| {
                if cx.is_fullscreen() {
                    this.pl_2()
                } else if self.platform_style == PlatformStyle::Mac {
                    this.pl(px(platform_mac::TRAFFIC_LIGHT_PADDING))
                } else {
                    this.pl_2()
                }
            })
            .map(|el| match decorations {
                Decorations::Server => el,
                Decorations::Client { tiling, .. } => el
                    .when(!(tiling.top || tiling.right), |el| {
                        el.rounded_tr(theme::CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.top || tiling.left), |el| {
                        el.rounded_tl(theme::CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    // this border is to avoid a transparent gap in the rounded corners
                    .mt(px(-1.))
                    .border(px(1.))
                    .border_color(titlebar_color),
            })
            .bg(titlebar_color)
            .content_stretch()
            .child(
                div()
                    .id("titlebar-content")
                    .flex()
                    .flex_row()
                    .justify_between()
                    .w_full()
                    // Note: On Windows the title bar behavior is handled by the platform implementation.
                    .when(self.platform_style != PlatformStyle::Windows, |this| {
                        this.on_click(|event, cx| {
                            if event.up.click_count == 2 {
                                cx.zoom_window();
                            }
                        })
                    })
                    .child(
                        h_flex()
                            .gap_1()
                            .when_some(self.application_menu.clone(), |this, menu| this.child(menu))
                            .children(self.render_project_host(model, cx))
                            .child(self.render_project_name(model, cx))
                            .children(self.render_project_branch(model, cx))
                            .on_mouse_down(MouseButton::Left, |_, cx| cx.stop_propagation()),
                    )
                    .child(self.render_collaborator_list(model, cx))
                    .child(
                        h_flex()
                            .gap_1()
                            .pr_1()
                            .on_mouse_down(MouseButton::Left, |_, cx| cx.stop_propagation())
                            .children(self.render_call_controls(model, cx))
                            .map(|el| {
                                let status = self.client.status();
                                let status = &*status.borrow();
                                if matches!(status, client::Status::Connected { .. }) {
                                    el.child(self.render_user_menu_button(model, cx))
                                } else {
                                    el.children(self.render_connection_status(status, model, cx))
                                        .child(self.render_sign_in_button(model, cx))
                                        .child(self.render_user_menu_button(model, cx))
                                }
                            }),
                    ),
            )
            .when(!cx.is_fullscreen(), |title_bar| match self.platform_style {
                PlatformStyle::Mac => title_bar,
                PlatformStyle::Linux => {
                    if matches!(decorations, Decorations::Client { .. }) {
                        title_bar
                            .child(platform_linux::LinuxWindowControls::new(close_action))
                            .when(supported_controls.window_menu, |titlebar| {
                                titlebar.on_mouse_down(gpui::MouseButton::Right, move |ev, cx| {
                                    cx.show_window_menu(ev.position)
                                })
                            })
                            .on_mouse_move(model.listener(move |this, _ev, cx| {
                                if this.should_move {
                                    this.should_move = false;
                                    cx.start_window_move();
                                }
                            }))
                            .on_mouse_down_out(model.listener(move |this, _ev, _cx| {
                                this.should_move = false;
                            }))
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                model.listener(move |this, _ev, _cx| {
                                    this.should_move = false;
                                }),
                            )
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                model.listener(move |this, _ev, _cx| {
                                    this.should_move = true;
                                }),
                            )
                    } else {
                        title_bar
                    }
                }
                PlatformStyle::Windows => {
                    title_bar.child(platform_windows::WindowsWindowControls::new(height))
                }
            })
    }
}

impl TitleBar {
    pub fn new(
        id: impl Into<ElementId>,
        workspace: &Workspace,
        model: &Model<Self>,
        cx: &mut AppContext,
    ) -> Self {
        let project = workspace.project().clone();
        let user_store = workspace.app_state().user_store.clone();
        let client = workspace.app_state().client.clone();
        let active_call = ActiveCall::global(cx);

        let platform_style = PlatformStyle::platform();
        let application_menu = match platform_style {
            PlatformStyle::Mac => None,
            PlatformStyle::Linux | PlatformStyle::Windows => {
                Some(cx.new_model(ApplicationMenu::new))
            }
        };

        let mut subscriptions = Vec::new();
        subscriptions.push(
            cx.observe(&workspace.weak_handle().upgrade().unwrap(), |_, _, cx| {
                model.notify(cx)
            }),
        );
        subscriptions.push(cx.observe(&project, |_, _, cx| model.notify(cx)));
        subscriptions.push(cx.observe(&active_call, |this, _, cx| this.active_call_changed(cx)));
        subscriptions.push(cx.observe_window_activation(Self::window_activation_changed));
        subscriptions.push(cx.observe(&user_store, |_, _, cx| model.notify(cx)));

        Self {
            platform_style,
            content: div().id(id.into()),
            children: SmallVec::new(),
            application_menu,
            workspace: workspace.weak_handle(),
            should_move: false,
            project,
            user_store,
            client,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn height(window: &mut gpui::Window, cx: &mut gpui::AppContext) -> Pixels {
        (1.75 * cx.rem_size()).max(px(34.))
    }

    #[cfg(target_os = "windows")]
    pub fn height(_window: &mut gpui::Window, _cx: &mut gpui::AppContext) -> Pixels {
        // todo(windows) instead of hard coded size report the actual size to the Windows platform API
        px(32.)
    }

    /// Sets the platform style.
    pub fn platform_style(mut self, style: PlatformStyle) -> Self {
        self.platform_style = style;
        self
    }

    fn render_ssh_project_host(
        &self,
        model: &Model<Self>,
        cx: &mut AppContext,
    ) -> Option<AnyElement> {
        let options = self.project.read(cx).ssh_connection_options(cx)?;
        let host: SharedString = options.connection_string().into();

        let nickname = options
            .nickname
            .clone()
            .map(|nick| nick.into())
            .unwrap_or_else(|| host.clone());

        let (indicator_color, meta) = match self.project.read(cx).ssh_connection_state(cx)? {
            remote::ConnectionState::Connecting => (Color::Info, format!("Connecting to: {host}")),
            remote::ConnectionState::Connected => (Color::Success, format!("Connected to: {host}")),
            remote::ConnectionState::HeartbeatMissed => (
                Color::Warning,
                format!("Connection attempt to {host} missed. Retrying..."),
            ),
            remote::ConnectionState::Reconnecting => (
                Color::Warning,
                format!("Lost connection to {host}. Reconnecting..."),
            ),
            remote::ConnectionState::Disconnected => {
                (Color::Error, format!("Disconnected from {host}"))
            }
        };

        let icon_color = match self.project.read(cx).ssh_connection_state(cx)? {
            remote::ConnectionState::Connecting => Color::Info,
            remote::ConnectionState::Connected => Color::Default,
            remote::ConnectionState::HeartbeatMissed => Color::Warning,
            remote::ConnectionState::Reconnecting => Color::Warning,
            remote::ConnectionState::Disconnected => Color::Error,
        };

        let meta = SharedString::from(meta);

        Some(
            ButtonLike::new("ssh-server-icon")
                .child(
                    IconWithIndicator::new(
                        Icon::new(IconName::Server)
                            .size(IconSize::XSmall)
                            .color(icon_color),
                        Some(Indicator::dot().color(indicator_color)),
                    )
                    .indicator_border_color(Some(cx.theme().colors().title_bar_background))
                    .into_any_element(),
                )
                .child(
                    div()
                        .max_w_32()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(Label::new(nickname.clone()).size(LabelSize::Small)),
                )
                .tooltip(move |window, cx| {
                    Tooltip::with_meta("Remote Project", Some(&OpenRemote), meta.clone(), model, cx)
                })
                .on_click(|_, cx| {
                    cx.dispatch_action(OpenRemote.boxed_clone());
                })
                .into_any_element(),
        )
    }

    pub fn render_project_host(
        &self,
        model: &Model<Self>,
        cx: &mut AppContext,
    ) -> Option<AnyElement> {
        if self.project.read(cx).is_via_ssh() {
            return self.render_ssh_project_host(model, cx);
        }

        if self.project.read(cx).is_disconnected(cx) {
            return Some(
                Button::new("disconnected", "Disconnected")
                    .disabled(true)
                    .color(Color::Disabled)
                    .style(ButtonStyle::Subtle)
                    .label_size(LabelSize::Small)
                    .into_any_element(),
            );
        }

        let host = self.project.read(cx).host()?;
        let host_user = self.user_store.read(cx).get_cached_user(host.user_id)?;
        let participant_index = self
            .user_store
            .read(cx)
            .participant_indices()
            .get(&host_user.id)?;
        Some(
            Button::new("project_owner_trigger", host_user.github_login.clone())
                .color(Color::Player(participant_index.0))
                .style(ButtonStyle::Subtle)
                .label_size(LabelSize::Small)
                .tooltip(move |window, cx| {
                    Tooltip::text(
                        format!(
                            "{} is sharing this project. Click to follow.",
                            host_user.github_login.clone()
                        ),
                        cx,
                    )
                })
                .on_click({
                    let host_peer_id = host.peer_id;
                    model.listener(move |this, _, cx| {
                        this.workspace
                            .update(cx, |workspace, model, cx| {
                                workspace.follow(host_peer_id, cx);
                            })
                            .log_err();
                    })
                })
                .into_any_element(),
        )
    }

    pub fn render_project_name(
        &self,
        model: &Model<Self>,
        cx: &mut AppContext,
    ) -> impl IntoElement {
        let name = {
            let mut names = self.project.read(cx).visible_worktrees(cx).map(|worktree| {
                let worktree = worktree.read(cx);
                worktree.root_name()
            });

            names.next()
        };
        let is_project_selected = name.is_some();
        let name = if let Some(name) = name {
            util::truncate_and_trailoff(name, MAX_PROJECT_NAME_LENGTH)
        } else {
            "Open recent project".to_string()
        };

        Button::new("project_name_trigger", name)
            .when(!is_project_selected, |b| b.color(Color::Muted))
            .style(ButtonStyle::Subtle)
            .label_size(LabelSize::Small)
            .tooltip(move |window, cx| {
                Tooltip::for_action(
                    "Recent Projects",
                    &zed_actions::OpenRecent {
                        create_new_window: false,
                    },
                    model,
                    cx,
                )
            })
            .on_click(model.listener(move |_, _, model, window, cx| {
                cx.dispatch_action(Box::new(OpenRecent {
                    create_new_window: false,
                }));
            }))
    }

    pub fn render_project_branch(
        &self,
        model: &Model<Self>,
        cx: &mut AppContext,
    ) -> Option<impl IntoElement> {
        let entry = {
            let mut names_and_branches =
                self.project.read(cx).visible_worktrees(cx).map(|worktree| {
                    let worktree = worktree.read(cx);
                    worktree.root_git_entry()
                });

            names_and_branches.next().flatten()
        };
        let workspace = self.workspace.upgrade()?;
        let branch_name = entry
            .as_ref()
            .and_then(RepositoryEntry::branch)
            .map(|branch| util::truncate_and_trailoff(&branch, MAX_BRANCH_NAME_LENGTH))?;
        Some(
            Button::new("project_branch_trigger", branch_name)
                .color(Color::Muted)
                .style(ButtonStyle::Subtle)
                .label_size(LabelSize::Small)
                .tooltip(move |window, cx| {
                    Tooltip::with_meta(
                        "Recent Branches",
                        Some(&zed_actions::branches::OpenRecent),
                        "Local branches only",
                        model,
                        cx,
                    )
                })
                .on_click(move |_, cx| {
                    let _ = workspace.update(cx, |_this, model, cx| {
                        cx.dispatch_action(zed_actions::branches::OpenRecent.boxed_clone());
                    });
                }),
        )
    }

    fn window_activation_changed(&mut self, model: &Model<Self>, cx: &mut AppContext) {
        if cx.is_window_active() {
            ActiveCall::global(cx)
                .update(cx, |call, model, cx| {
                    call.set_location(Some(&self.project), model, cx)
                })
                .detach_and_log_err(cx);
        } else if cx.active_window().is_none() {
            ActiveCall::global(cx)
                .update(cx, |call, model, cx| call.set_location(None, model, cx))
                .detach_and_log_err(cx);
        }
        self.workspace
            .update(cx, |workspace, model, cx| {
                workspace.update_active_view_for_followers(cx);
            })
            .ok();
    }

    fn active_call_changed(&mut self, model: &Model<Self>, cx: &mut AppContext) {
        model.notify(cx);
    }

    fn share_project(&mut self, _: &ShareProject, model: &Model<Self>, cx: &mut AppContext) {
        let active_call = ActiveCall::global(cx);
        let project = self.project.clone();
        active_call
            .update(cx, |call, model, cx| call.share_project(project, model, cx))
            .detach_and_log_err(cx);
    }

    fn unshare_project(&mut self, _: &UnshareProject, model: &Model<Self>, cx: &mut AppContext) {
        let active_call = ActiveCall::global(cx);
        let project = self.project.clone();
        active_call
            .update(cx, |call, model, cx| {
                call.unshare_project(project, model, cx)
            })
            .log_err();
    }

    fn render_connection_status(
        &self,
        status: &client::Status,
        model: &Model<Self>,
        cx: &mut AppContext,
    ) -> Option<AnyElement> {
        match status {
            client::Status::ConnectionError
            | client::Status::ConnectionLost
            | client::Status::Reauthenticating { .. }
            | client::Status::Reconnecting { .. }
            | client::Status::ReconnectionError { .. } => Some(
                div()
                    .id("disconnected")
                    .child(Icon::new(IconName::Disconnected).size(IconSize::Small))
                    .tooltip(|window, cx| Tooltip::text("Disconnected", cx))
                    .into_any_element(),
            ),
            client::Status::UpgradeRequired => {
                let auto_updater = auto_update::AutoUpdater::get(cx);
                let label = match auto_updater.map(|auto_update| auto_update.read(cx).status()) {
                    Some(AutoUpdateStatus::Updated { .. }) => "Please restart Zed to Collaborate",
                    Some(AutoUpdateStatus::Installing)
                    | Some(AutoUpdateStatus::Downloading)
                    | Some(AutoUpdateStatus::Checking) => "Updating...",
                    Some(AutoUpdateStatus::Idle) | Some(AutoUpdateStatus::Errored) | None => {
                        "Please update Zed to Collaborate"
                    }
                };

                Some(
                    Button::new("connection-status", label)
                        .label_size(LabelSize::Small)
                        .on_click(|_, cx| {
                            if let Some(auto_updater) = auto_update::AutoUpdater::get(cx) {
                                if auto_updater.read(cx).status().is_updated() {
                                    workspace::reload(&Default::default(), cx);
                                    return;
                                }
                            }
                            auto_update::check(&Default::default(), model, cx);
                        })
                        .into_any_element(),
                )
            }
            _ => None,
        }
    }

    pub fn render_sign_in_button(&mut self, _: &Model<Self>, _: &mut AppContext) -> Button {
        let client = self.client.clone();
        Button::new("sign_in", "Sign in")
            .label_size(LabelSize::Small)
            .on_click(move |_, cx| {
                let client = client.clone();
                cx.spawn(move |mut cx| async move {
                    client
                        .authenticate_and_connect(true, &cx)
                        .await
                        .notify_async_err(model, &mut cx);
                })
                .detach();
            })
    }

    pub fn render_user_menu_button(
        &mut self,
        model: &Model<Self>,
        cx: &mut AppContext,
    ) -> impl Element {
        let user_store = self.user_store.read(cx);
        if let Some(user) = user_store.current_user() {
            let plan = user_store.current_plan();
            PopoverMenu::new("user-menu")
                .menu(move |window, cx| {
                    ContextMenu::build(cx, window, |menu, model, window, cx| {
                        menu.when(cx.has_flag::<ZedPro>(), |menu| {
                            menu.action(
                                format!(
                                    "Current Plan: {}",
                                    match plan {
                                        None => "",
                                        Some(proto::Plan::Free) => "Free",
                                        Some(proto::Plan::ZedPro) => "Pro",
                                    }
                                ),
                                zed_actions::OpenAccountSettings.boxed_clone(),
                            )
                            .separator()
                        })
                        .action("Settings", zed_actions::OpenSettings.boxed_clone())
                        .action("Key Bindings", Box::new(zed_actions::OpenKeymap))
                        .action(
                            "Themes…",
                            zed_actions::theme_selector::Toggle::default().boxed_clone(),
                        )
                        .action("Extensions", zed_actions::Extensions.boxed_clone())
                        .separator()
                        .link(
                            "Book Onboarding",
                            OpenBrowser {
                                url: BOOK_ONBOARDING.to_string(),
                            }
                            .boxed_clone(),
                        )
                        .action("Sign Out", client::SignOut.boxed_clone())
                    })
                    .into()
                })
                .trigger(
                    ButtonLike::new("user-menu")
                        .child(
                            h_flex()
                                .gap_0p5()
                                .children(
                                    workspace::WorkspaceSettings::get_global(cx)
                                        .show_user_picture
                                        .then(|| Avatar::new(user.avatar_uri.clone())),
                                )
                                .child(
                                    Icon::new(IconName::ChevronDown)
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                ),
                        )
                        .style(ButtonStyle::Subtle)
                        .tooltip(move |window, cx| Tooltip::text("Toggle User Menu", cx)),
                )
                .anchor(gpui::AnchorCorner::TopRight)
        } else {
            PopoverMenu::new("user-menu")
                .menu(|window, cx| {
                    ContextMenu::build(cx, window, |menu, model, window, cx| {
                        menu.action("Settings", zed_actions::OpenSettings.boxed_clone())
                            .action("Key Bindings", Box::new(zed_actions::OpenKeymap))
                            .action(
                                "Themes…",
                                zed_actions::theme_selector::Toggle::default().boxed_clone(),
                            )
                            .action("Extensions", zed_actions::Extensions.boxed_clone())
                            .separator()
                            .link(
                                "Book Onboarding",
                                OpenBrowser {
                                    url: BOOK_ONBOARDING.to_string(),
                                }
                                .boxed_clone(),
                            )
                    })
                    .into()
                })
                .trigger(
                    ButtonLike::new("user-menu")
                        .child(
                            h_flex().gap_0p5().child(
                                Icon::new(IconName::ChevronDown)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            ),
                        )
                        .style(ButtonStyle::Subtle)
                        .tooltip(move |window, cx| Tooltip::text("Toggle User Menu", cx)),
                )
        }
    }
}

impl InteractiveElement for TitleBar {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.content.interactivity()
    }
}

impl StatefulInteractiveElement for TitleBar {}

impl ParentElement for TitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

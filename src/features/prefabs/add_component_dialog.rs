use gpui::{prelude::*, *};
use pulsar_reflection::REGISTRY;
use ui::{
    dropdown::{SearchableList, SearchableListEvent},
    IconName,
};

#[derive(Debug, Clone)]
pub struct PrefabComponentSelectedEvent {
    pub class_name: String,
}

pub struct AddPrefabComponentDialog {
    searchable_list: Entity<SearchableList<&'static str>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DismissEvent> for AddPrefabComponentDialog {}
impl EventEmitter<PrefabComponentSelectedEvent> for AddPrefabComponentDialog {}

impl Focusable for AddPrefabComponentDialog {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.searchable_list.read(cx).focus_handle(cx)
    }
}

impl AddPrefabComponentDialog {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut engine_classes = REGISTRY.get_class_names();
        engine_classes.sort();

        let searchable_list = cx.new(|cx| {
            SearchableList::new(window, cx, engine_classes.clone(), |name| name.to_string())
                .with_empty_text("No components found")
                .with_max_width(px(260.0))
                .with_max_height(px(320.0))
                .with_icon_getter(|_| IconName::Component)
        });

        let subscriptions = vec![cx.subscribe(
            &searchable_list,
            |this, _, event: &SearchableListEvent<&'static str>, cx| {
                if let SearchableListEvent::Select(class_name) = event {
                    this.select_component(class_name, cx);
                }
            },
        )];

        Self {
            searchable_list,
            _subscriptions: subscriptions,
        }
    }

    fn select_component(&self, class_name: &str, cx: &mut Context<Self>) {
        if REGISTRY.has_class(class_name) {
            cx.emit(PrefabComponentSelectedEvent {
                class_name: class_name.to_string(),
            });
        }
        cx.emit(DismissEvent);
    }
}

impl Render for AddPrefabComponentDialog {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.searchable_list.clone()
    }
}

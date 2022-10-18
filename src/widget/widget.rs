// Copyright 2018 The Druid Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::any::Any;
use std::num::NonZeroU64;
use std::ops::{Deref, DerefMut};

use smallvec::SmallVec;
use tracing::{trace_span, Span};

use super::prelude::*;
use crate::contexts::WidgetCtx;
use crate::widget::WidgetRef;
use crate::AsAny;
use crate::Point;

/// A unique identifier for a single [`Widget`].
///
/// `WidgetId`s are generated automatically for all widgets that participate
/// in layout. More specifically, each [`WidgetPod`] has a unique `WidgetId`.
///
/// These ids are used internally to route events, and can be used to communicate
/// between widgets, by submitting a command (as with [`EventCtx::submit_command`])
/// and passing a `WidgetId` as the [`Target`].
///
/// A widget can retrieve its id via methods on the various contexts, such as
/// [`LifeCycleCtx::widget_id`].
///
/// ## Explicit `WidgetId`s.
///
/// Sometimes, you may want to know a widget's id when constructing the widget.
/// You can give a widget an _explicit_ id by wrapping it in an [`IdentityWrapper`]
/// widget, or by using the [`WidgetExt::with_id`] convenience method.
///
/// If you set a `WidgetId` directly, you are resposible for ensuring that it
/// is unique in time. That is: only one widget can exist with a given id at a
/// given time.
///
/// [`Widget`]: trait.Widget.html
/// [`EventCtx::submit_command`]: struct.EventCtx.html#method.submit_command
/// [`Target`]: enum.Target.html
/// [`WidgetPod`]: struct.WidgetPod.html
/// [`LifeCycleCtx::widget_id`]: struct.LifeCycleCtx.html#method.widget_id
/// [`WidgetExt::with_id`]: trait.WidgetExt.html#method.with_id
/// [`IdentityWrapper`]: widget/struct.IdentityWrapper.html
// this is NonZeroU64 because we regularly store Option<WidgetId>
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct WidgetId(NonZeroU64);

/// The trait implemented by all widgets.
///
/// All appearance and behavior for a widget is encapsulated in an
/// object that implements this trait.
///
/// The trait is parametrized by a type (`T`) for associated data.
/// All trait methods are provided with access to this data, and
/// in the case of [`event`] the reference is mutable, so that events
/// can directly update the data.
///
/// Whenever the application data changes, the framework traverses
/// the widget hierarchy with an [`update`] method. The framework
/// needs to know whether the data has actually changed or not, which
/// is why `T` has a [`Data`] bound.
///
/// All the trait methods are provided with a corresponding context.
/// The widget can request things and cause actions by calling methods
/// on that context.
///
/// In addition, all trait methods are provided with an environment
/// ([`Env`]).
///
/// Container widgets will generally not call `Widget` methods directly
/// on their child widgets, but rather will own their widget wrapped in
/// a [`WidgetPod`], and call the corresponding method on that. The
/// `WidgetPod` contains state and logic for these traversals. On the
/// other hand, particularly light-weight containers might contain their
/// child `Widget` directly (when no layout or event flow logic is
/// needed), and in those cases will call these methods.
///
/// As a general pattern, container widgets will call the corresponding
/// `WidgetPod` method on all their children. The `WidgetPod` applies
/// logic to determine whether to recurse, as needed.
///
/// [`event`]: #tymethod.event
/// [`update`]: #tymethod.update
/// [`Data`]: trait.Data.html
/// [`Env`]: struct.Env.html
/// [`WidgetPod`]: struct.WidgetPod.html
pub trait Widget: AsAny {
    /// Handle an event.
    ///
    /// A number of different events (in the [`Event`] enum) are handled in this
    /// method call. A widget can handle these events in a number of ways:
    /// requesting things from the [`EventCtx`], mutating the data, or submitting
    /// a [`Command`].
    ///
    /// [`Event`]: enum.Event.html
    /// [`EventCtx`]: struct.EventCtx.html
    /// [`Command`]: struct.Command.html
    fn on_event(&mut self, ctx: &mut EventCtx, event: &Event, env: &Env);

    fn on_status_change(&mut self, ctx: &mut LifeCycleCtx, event: &StatusChange, env: &Env);

    /// Handle a life cycle notification.
    ///
    /// This method is called to notify your widget of certain special events,
    /// (available in the [`LifeCycle`] enum) that are generally related to
    /// changes in the widget graph or in the state of your specific widget.
    ///
    /// A widget is not expected to mutate the application state in response
    /// to these events, but only to update its own internal state as required;
    /// if a widget needs to mutate data, it can submit a [`Command`] that will
    /// be executed at the next opportunity.
    ///
    /// [`LifeCycle`]: enum.LifeCycle.html
    /// [`LifeCycleCtx`]: struct.LifeCycleCtx.html
    /// [`Command`]: struct.Command.html
    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, env: &Env);

    /// Compute layout.
    ///
    /// A leaf widget should determine its size (subject to the provided
    /// constraints) and return it.
    ///
    /// A container widget will recursively call [`WidgetPod::layout`] on its
    /// child widgets, providing each of them an appropriate box constraint,
    /// compute layout, then call [`set_origin`] on each of its children.
    /// Finally, it should return the size of the container. The container
    /// can recurse in any order, which can be helpful to, for example, compute
    /// the size of non-flex widgets first, to determine the amount of space
    /// available for the flex widgets.
    ///
    /// For efficiency, a container should only invoke layout of a child widget
    /// once, though there is nothing enforcing this.
    ///
    /// The layout strategy is strongly inspired by Flutter.
    ///
    /// [`WidgetPod::layout`]: struct.WidgetPod.html#method.layout
    /// [`set_origin`]: struct.WidgetPod.html#method.set_origin
    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, env: &Env) -> Size;

    /// Paint the widget appearance.
    ///
    /// The [`PaintCtx`] derefs to something that implements the [`RenderContext`]
    /// trait, which exposes various methods that the widget can use to paint
    /// its appearance.
    ///
    /// Container widgets can paint a background before recursing to their
    /// children, or annotations (for example, scrollbars) by painting
    /// afterwards. In addition, they can apply masks and transforms on
    /// the render context, which is especially useful for scrolling.
    ///
    /// [`PaintCtx`]: struct.PaintCtx.html
    /// [`RenderContext`]: trait.RenderContext.html
    fn paint(&mut self, ctx: &mut PaintCtx, env: &Env);

    fn children(&self) -> SmallVec<[WidgetRef<'_, dyn Widget>; 16]>;

    fn make_trace_span(&self) -> Span {
        trace_span!("Widget", r#type = self.short_type_name())
    }

    fn get_debug_text(&self) -> Option<String> {
        None
    }

    // --- Auto-generated implementations ---

    // Returns direct child, not recursive child
    fn get_child_at_pos(&self, pos: Point) -> Option<WidgetRef<'_, dyn Widget>> {
        // layout_rect() is in parent coordinate space
        self.children()
            .into_iter()
            .find(|child| child.state().layout_rect().contains(pos))
    }

    #[doc(hidden)]
    /// Get the (verbose) type name of the widget for debugging purposes.
    /// You should not override this method.
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    #[doc(hidden)]
    /// Get the (abridged) type name of the widget for debugging purposes.
    /// You should not override this method.
    fn short_type_name(&self) -> &'static str {
        let name = self.type_name();
        name.split('<')
            .next()
            .unwrap_or(name)
            .split("::")
            .last()
            .unwrap_or(name)
    }

    #[doc(hidden)]
    fn as_any(&self) -> &dyn Any {
        self.as_dyn_any()
    }

    #[doc(hidden)]
    fn as_mut_any(&mut self) -> &mut dyn Any {
        self.as_mut_dyn_any()
    }
}

pub trait StoreInWidgetMut: Widget {
    type Mut<'a, 'b: 'a>: std::ops::Deref<Target = Self>;

    fn from_widget_and_ctx<'a, 'b>(
        widget: &'a mut Self,
        ctx: WidgetCtx<'a, 'b>,
    ) -> Self::Mut<'a, 'b>;

    fn get_widget<'s: 'r, 'a: 'r, 'b: 'a, 'r>(
        widget_mut: &'s mut Self::Mut<'a, 'b>,
    ) -> &'r mut Self {
        Self::get_widget_and_ctx(widget_mut).0
    }

    fn get_ctx<'s: 'r, 'a: 'r, 'b: 'a, 'r>(
        widget_mut: &'s mut Self::Mut<'a, 'b>,
    ) -> &'r mut WidgetCtx<'a, 'b> {
        Self::get_widget_and_ctx(widget_mut).1
    }

    fn get_widget_and_ctx<'s: 'r, 'a: 'r, 'b: 'a, 'r>(
        widget_mut: &'s mut Self::Mut<'a, 'b>,
    ) -> (&'r mut Self, &'r mut WidgetCtx<'a, 'b>);
}

#[macro_export]
macro_rules! declare_widget {
    ($WidgetNameMut:ident, $WidgetName:ident) => {
        crate::declare_widget!($WidgetNameMut, $WidgetName<>);
    };

    ($WidgetNameMut:ident, $WidgetName:ident<$($Arg:ident $(: ($($Bound:tt)*))?),*>) => {
        pub struct $WidgetNameMut<'a, 'b, $($Arg $(: $($Bound)*)?),*>(WidgetCtx<'a, 'b>, &'a mut $WidgetName<$($Arg),*>);

        impl<$($Arg $(: $($Bound)*)?),*> crate::widget::StoreInWidgetMut for $WidgetName<$($Arg),*> {
            type Mut<'a, 'b: 'a> = $WidgetNameMut<'a, 'b, $($Arg),*>;

            fn get_widget_and_ctx<'s: 'r, 'a: 'r, 'b: 'a, 'r>(
                widget_mut: &'s mut Self::Mut<'a, 'b>,
            ) -> (&'r mut Self, &'r mut WidgetCtx<'a, 'b>) {
                (widget_mut.1, &mut widget_mut.0)
            }

            fn from_widget_and_ctx<'a, 'b>(
                widget: &'a mut Self,
                ctx: WidgetCtx<'a, 'b>,
            ) -> Self::Mut<'a, 'b> {
                $WidgetNameMut(ctx, widget)
            }
        }

        impl<'a, 'b, $($Arg $(: $($Bound)*)?),*> ::std::ops::Deref for $WidgetNameMut<'a, 'b, $($Arg),*> {
            type Target = $WidgetName<$($Arg),*>;

            fn deref(&self) -> &Self::Target {
                self.1
            }
        }
    };
}

#[cfg(not(tarpaulin_include))]
impl WidgetId {
    /// Allocate a new, unique `WidgetId`.
    ///
    /// All widgets are assigned ids automatically; you should only create
    /// an explicit id if you need to know it ahead of time, for instance
    /// if you want two sibling widgets to know each others' ids.
    ///
    /// You must ensure that a given `WidgetId` is only ever used for one
    /// widget at a time.
    pub fn next() -> WidgetId {
        use druid_shell::Counter;
        static WIDGET_ID_COUNTER: Counter = Counter::new();
        WidgetId(WIDGET_ID_COUNTER.next_nonzero())
    }

    /// Create a reserved `WidgetId`, suitable for reuse.
    ///
    /// The caller is responsible for ensuring that this ID is in fact assigned
    /// to a single widget at any time, or your code may become haunted.
    ///
    /// The actual inner representation of the returned `WidgetId` will not
    /// be the same as the raw value that is passed in; it will be
    /// `u64::max_value() - raw`.
    #[allow(unsafe_code)]
    pub const fn reserved(raw: u16) -> WidgetId {
        let id = u64::max_value() - raw as u64;
        // safety: by construction this can never be zero.
        WidgetId(unsafe { std::num::NonZeroU64::new_unchecked(id) })
    }

    pub(crate) fn to_raw(self) -> u64 {
        self.0.into()
    }
}

// TODO - remove
impl Widget for Box<dyn Widget> {
    fn on_event(&mut self, ctx: &mut EventCtx, event: &Event, env: &Env) {
        self.deref_mut().on_event(ctx, event, env)
    }

    fn on_status_change(&mut self, ctx: &mut LifeCycleCtx, event: &StatusChange, env: &Env) {
        self.deref_mut().on_status_change(ctx, event, env)
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, env: &Env) {
        self.deref_mut().lifecycle(ctx, event, env);
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, env: &Env) -> Size {
        self.deref_mut().layout(ctx, bc, env)
    }

    fn paint(&mut self, ctx: &mut PaintCtx, env: &Env) {
        self.deref_mut().paint(ctx, env);
    }

    fn type_name(&self) -> &'static str {
        self.deref().type_name()
    }

    fn children(&self) -> SmallVec<[WidgetRef<'_, dyn Widget>; 16]> {
        self.deref().children()
    }

    fn make_trace_span(&self) -> Span {
        self.deref().make_trace_span()
    }

    fn get_debug_text(&self) -> Option<String> {
        self.deref().get_debug_text()
    }

    fn as_any(&self) -> &dyn Any {
        self.deref().as_dyn_any()
    }

    fn as_mut_any(&mut self) -> &mut dyn Any {
        self.deref_mut().as_mut_dyn_any()
    }
}

// We use alias type because macro doesn't accept braces except in some cases.
type BoxWidget = Box<dyn Widget>;
crate::declare_widget!(BoxWidgetMut, BoxWidget);
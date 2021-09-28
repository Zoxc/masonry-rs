// Copyright 2019 The Druid Authors.
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

//! A label widget.

use smallvec::SmallVec;
use std::ops::{Deref, DerefMut};

use druid_shell::Cursor;

use crate::kurbo::Vec2;
use crate::text::{TextAlignment, TextLayout};
use crate::widget::prelude::*;
use crate::{ArcStr, Color, Data, FontDescriptor, KeyOrValue, Point};
use tracing::{instrument, trace};

// added padding between the edges of the widget and the text.
const LABEL_X_PADDING: f64 = 2.0;

/// A label that displays static or dynamic text.
///
/// This type manages an inner [`RawLabel`], updating its text based on the
/// current [`Data`] and [`Env`] as required.
///
/// If your [`Data`] is *already* text, you may use a [`RawLabel`] directly.
/// As a convenience, you can create a [`RawLabel`] with the [`Label::raw`]
/// constructor method.
///
/// A label is the easiest way to display text in Druid. A label is instantiated
/// with some [`LabelText`] type, such as an [`ArcStr`] or a [`LocalizedString`],
/// and also has methods for setting the default font, font-size, text color,
/// and other attributes.
///
/// In addition to being a [`Widget`], `Label` is also regularly used as a
/// component in other widgets that wish to display text; to facilitate this
/// it has a [`draw_at`] method that allows the caller to easily draw the label's
/// text at the desired position on screen.
///
/// # Examples
///
/// Make a label to say something **very** important:
///
/// ```
/// # use druid::widget::{Label, SizedBox};
/// # use druid::*;
///
/// let font = FontDescriptor::new(FontFamily::SYSTEM_UI)
///     .with_weight(FontWeight::BOLD)
///     .with_size(48.0);
///
/// let important_label = Label::new("WATCH OUT!")
///     .with_font(font)
///     .with_text_color(Color::rgb(1.0, 0.2, 0.2));
/// # // our data type T isn't known; this is just a trick for the compiler
/// # // to keep our example clean
/// # let _ = SizedBox::<()>::new(important_label);
/// ```
///
/// [`ArcStr`]: ../type.ArcStr.html
/// [`Data`]: ../trait.Data.html
/// [`Env`]: ../struct.Env.html
/// [`RawLabel`]: struct.RawLabel.html
/// [`Label::raw`]: #method.raw
/// [`LabelText`]: struct.LabelText.html
/// [`LocalizedString`]: ../struct.LocalizedString.html
/// [`draw_at`]: #method.draw_at
/// [`Widget`]: ../trait.Widget.html
pub struct Label {
    label: RawLabel,
    current_text: ArcStr,
    text: LabelText,
    // for debuging, we track if the user modifies the text and we don't get
    // an update call, which might cause us to display stale text.
    text_should_be_updated: bool,
}

/// A widget that displays text data.
///
/// This requires the `Data` to implement [`TextStorage`]; to handle static, dynamic, or
/// localized text, use [`Label`].
pub struct RawLabel {
    layout: TextLayout<ArcStr>,
    line_break_mode: LineBreaking,

    disabled: bool,
    default_text_color: KeyOrValue<Color>,
}

/// Options for handling lines that are too wide for the label.
#[derive(Debug, Clone, Copy, PartialEq, Data)]
pub enum LineBreaking {
    /// Lines are broken at word boundaries.
    WordWrap,
    /// Lines are truncated to the width of the label.
    Clip,
    /// Lines overflow the label.
    Overflow,
}

/// The text for a [`Label`].
///
/// This can be one of three things; either an [`ArcStr`], a [`LocalizedString`],
/// or a closure with the signature, `Fn(&T, &Env) -> String`, where `T` is
/// the `Data` at this point in the tree.
///
/// [`ArcStr`]: ../type.ArcStr.html
/// [`LocalizedString`]: ../struct.LocalizedString.html
/// [`Label`]: struct.Label.html
#[derive(Clone)]
pub enum LabelText {
    /// Static text.
    Static(Static),
}

/// Static text.
#[derive(Debug, Clone)]
pub struct Static {
    /// The text.
    string: ArcStr,
    /// Whether or not the `resolved` method has been called yet.
    ///
    /// We want to return `true` from that method when it is first called,
    /// so that callers will know to retrieve the text. This matches
    /// the behaviour of the other variants.
    resolved: bool,
}

impl RawLabel {
    /// Create a new `RawLabel`.
    pub fn new() -> Self {
        Self {
            layout: TextLayout::new(),
            line_break_mode: LineBreaking::Overflow,
            disabled: false,
            default_text_color: crate::theme::TEXT_COLOR.into(),
        }
    }

    /// Builder-style method for setting the text color.
    ///
    /// The argument can be either a `Color` or a [`Key<Color>`].
    ///
    /// [`Key<Color>`]: ../struct.Key.html
    pub fn with_text_color(mut self, color: impl Into<KeyOrValue<Color>>) -> Self {
        self.set_text_color(color);
        self
    }

    /// Builder-style method for setting the text size.
    ///
    /// The argument can be either an `f64` or a [`Key<f64>`].
    ///
    /// [`Key<f64>`]: ../struct.Key.html
    pub fn with_text_size(mut self, size: impl Into<KeyOrValue<f64>>) -> Self {
        self.set_text_size(size);
        self
    }

    /// Builder-style method for setting the font.
    ///
    /// The argument can be a [`FontDescriptor`] or a [`Key<FontDescriptor>`]
    /// that refers to a font defined in the [`Env`].
    ///
    /// [`Env`]: ../struct.Env.html
    /// [`FontDescriptor`]: ../struct.FontDescriptor.html
    /// [`Key<FontDescriptor>`]: ../struct.Key.html
    pub fn with_font(mut self, font: impl Into<KeyOrValue<FontDescriptor>>) -> Self {
        self.set_font(font);
        self
    }

    /// Builder-style method to set the [`LineBreaking`] behaviour.
    ///
    /// [`LineBreaking`]: enum.LineBreaking.html
    pub fn with_line_break_mode(mut self, mode: LineBreaking) -> Self {
        self.set_line_break_mode(mode);
        self
    }

    /// Builder-style method to set the [`TextAlignment`].
    ///
    /// [`TextAlignment`]: enum.TextAlignment.html
    pub fn with_text_alignment(mut self, alignment: TextAlignment) -> Self {
        self.set_text_alignment(alignment);
        self
    }

    /// Set the text.
    pub fn set_text(&mut self, new_text: impl Into<ArcStr>) {
        self.layout.set_text(new_text.into());
    }

    /// Set the text color.
    ///
    /// The argument can be either a `Color` or a [`Key<Color>`].
    ///
    /// If you change this property, you are responsible for calling
    /// [`request_layout`] to ensure the label is updated.
    ///
    /// [`request_layout`]: ../struct.EventCtx.html#method.request_layout
    /// [`Key<Color>`]: ../struct.Key.html
    pub fn set_text_color(&mut self, color: impl Into<KeyOrValue<Color>>) {
        let color = color.into();
        if !self.disabled {
            self.layout.set_text_color(color.clone());
        }
        self.default_text_color = color;
    }

    /// Set the text size.
    ///
    /// The argument can be either an `f64` or a [`Key<f64>`].
    ///
    /// If you change this property, you are responsible for calling
    /// [`request_layout`] to ensure the label is updated.
    ///
    /// [`request_layout`]: ../struct.EventCtx.html#method.request_layout
    /// [`Key<f64>`]: ../struct.Key.html
    pub fn set_text_size(&mut self, size: impl Into<KeyOrValue<f64>>) {
        self.layout.set_text_size(size);
    }

    /// Set the font.
    ///
    /// The argument can be a [`FontDescriptor`] or a [`Key<FontDescriptor>`]
    /// that refers to a font defined in the [`Env`].
    ///
    /// If you change this property, you are responsible for calling
    /// [`request_layout`] to ensure the label is updated.
    ///
    /// [`request_layout`]: ../struct.EventCtx.html#method.request_layout
    /// [`Env`]: ../struct.Env.html
    /// [`FontDescriptor`]: ../struct.FontDescriptor.html
    /// [`Key<FontDescriptor>`]: ../struct.Key.html
    pub fn set_font(&mut self, font: impl Into<KeyOrValue<FontDescriptor>>) {
        self.layout.set_font(font);
    }

    /// Set the [`LineBreaking`] behaviour.
    ///
    /// If you change this property, you are responsible for calling
    /// [`request_layout`] to ensure the label is updated.
    ///
    /// [`request_layout`]: ../struct.EventCtx.html#method.request_layout
    /// [`LineBreaking`]: enum.LineBreaking.html
    pub fn set_line_break_mode(&mut self, mode: LineBreaking) {
        self.line_break_mode = mode;
    }

    /// Set the [`TextAlignment`] for this layout.
    ///
    /// [`TextAlignment`]: enum.TextAlignment.html
    pub fn set_text_alignment(&mut self, alignment: TextAlignment) {
        self.layout.set_text_alignment(alignment);
    }

    /// Draw this label's text at the provided `Point`, without internal padding.
    ///
    /// This is a convenience for widgets that want to use Label as a way
    /// of managing a dynamic or localized string, but want finer control
    /// over where the text is drawn.
    pub fn draw_at(&self, ctx: &mut PaintCtx, origin: impl Into<Point>) {
        self.layout.draw(ctx, origin)
    }

    /// Return the offset of the first baseline relative to the bottom of the widget.
    pub fn baseline_offset(&self) -> f64 {
        let text_metrics = self.layout.layout_metrics();
        text_metrics.size.height - text_metrics.first_baseline
    }
}

impl Label {
    /// Create a new [`RawLabel`].
    ///
    /// This can display text `Data` directly.
    pub fn raw() -> RawLabel {
        RawLabel::new()
    }
}

impl Label {
    /// Construct a new `Label` widget.
    ///
    /// ```
    /// use druid::LocalizedString;
    /// use druid::widget::Label;
    ///
    /// // Construct a new Label using static string.
    /// let _: Label<u32> = Label::new("Hello world");
    ///
    /// // Construct a new Label using localized string.
    /// let text = LocalizedString::new("hello-counter").with_arg("count", |data: &u32, _env| (*data).into());
    /// let _: Label<u32> = Label::new(text);
    ///
    /// // Construct a new dynamic Label. Text will be updated when data changes.
    /// let _: Label<u32> = Label::new(|data: &u32, _env: &_| format!("Hello world: {}", data));
    /// ```
    pub fn new(text: impl Into<LabelText>) -> Self {
        let text = text.into();
        let current_text = text.display_text();
        let mut label = RawLabel::new();
        label.set_text(current_text.clone());
        Self {
            text,
            current_text,
            label,
            text_should_be_updated: false,
        }
    }

    /// Return the current value of the label's text.
    pub fn text(&self) -> ArcStr {
        self.text.display_text()
    }

    /// Set the label's text.
    ///
    /// # Note
    ///
    /// If you change this property, at runtime, you **must** ensure that [`update`]
    /// is called in order to correctly recompute the text. If you are unsure,
    /// call [`request_update`] explicitly.
    ///
    /// [`update`]: ../trait.Widget.html#tymethod.update
    /// [`request_update`]: ../struct.EventCtx.html#method.request_update
    pub fn set_text(&mut self, text: impl Into<LabelText>) {
        self.text = text.into();
        self.text_should_be_updated = true;
    }

    /// Builder-style method for setting the text color.
    ///
    /// The argument can be either a `Color` or a [`Key<Color>`].
    ///
    /// [`Key<Color>`]: ../struct.Key.html
    pub fn with_text_color(mut self, color: impl Into<KeyOrValue<Color>>) -> Self {
        self.label.set_text_color(color);
        self
    }

    /// Builder-style method for setting the text size.
    ///
    /// The argument can be either an `f64` or a [`Key<f64>`].
    ///
    /// [`Key<f64>`]: ../struct.Key.html
    pub fn with_text_size(mut self, size: impl Into<KeyOrValue<f64>>) -> Self {
        self.label.set_text_size(size);
        self
    }

    /// Builder-style method for setting the font.
    ///
    /// The argument can be a [`FontDescriptor`] or a [`Key<FontDescriptor>`]
    /// that refers to a font defined in the [`Env`].
    ///
    /// [`Env`]: ../struct.Env.html
    /// [`FontDescriptor`]: ../struct.FontDescriptor.html
    /// [`Key<FontDescriptor>`]: ../struct.Key.html
    pub fn with_font(mut self, font: impl Into<KeyOrValue<FontDescriptor>>) -> Self {
        self.label.set_font(font);
        self
    }

    /// Builder-style method to set the [`LineBreaking`] behaviour.
    ///
    /// [`LineBreaking`]: enum.LineBreaking.html
    pub fn with_line_break_mode(mut self, mode: LineBreaking) -> Self {
        self.label.set_line_break_mode(mode);
        self
    }

    /// Builder-style method to set the [`TextAlignment`].
    ///
    /// [`TextAlignment`]: enum.TextAlignment.html
    pub fn with_text_alignment(mut self, alignment: TextAlignment) -> Self {
        self.label.set_text_alignment(alignment);
        self
    }

    /// Draw this label's text at the provided `Point`, without internal padding.
    ///
    /// This is a convenience for widgets that want to use Label as a way
    /// of managing a dynamic or localized string, but want finer control
    /// over where the text is drawn.
    pub fn draw_at(&self, ctx: &mut PaintCtx, origin: impl Into<Point>) {
        self.label.draw_at(ctx, origin)
    }
}

impl Static {
    fn new(s: ArcStr) -> Self {
        Static {
            string: s,
            resolved: false,
        }
    }

    fn resolve(&mut self) -> bool {
        let is_first_call = !self.resolved;
        self.resolved = true;
        is_first_call
    }
}

impl LabelText {
    /// Call callback with the text that should be displayed.
    pub fn with_display_text<V>(&self, mut cb: impl FnMut(&str) -> V) -> V {
        match self {
            LabelText::Static(s) => cb(&s.string),
        }
    }

    /// Return the current resolved text.
    pub fn display_text(&self) -> ArcStr {
        match self {
            LabelText::Static(s) => s.string.clone(),
        }
    }

    /// Update the localization, if necessary.
    /// This ensures that localized strings are up to date.
    ///
    /// Returns `true` if the string has changed.
    pub fn resolve(&mut self, env: &Env) -> bool {
        match self {
            LabelText::Static(s) => s.resolve(),
        }
    }
}

impl Widget for Label {
    #[instrument(name = "Label", level = "trace", skip(self, _ctx, _event, _env))]
    fn on_event(&mut self, _ctx: &mut EventCtx, _event: &Event, _env: &Env) {}

    #[instrument(name = "Label", level = "trace", skip(self, ctx, event, env))]
    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, env: &Env) {
        self.label.lifecycle(ctx, event, env);
    }

    #[instrument(name = "Label", level = "trace", skip(self, ctx, bc, env))]
    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, env: &Env) -> Size {
        self.label.layout(ctx, bc, env)
    }

    #[instrument(name = "Label", level = "trace", skip(self, ctx, env))]
    fn paint(&mut self, ctx: &mut PaintCtx, env: &Env) {
        if self.text_should_be_updated {
            tracing::warn!("Label text changed without call to update. See LabelAdapter::set_text for information.");
        }
        self.label.paint(ctx, env)
    }

    fn children(&self) -> SmallVec<[&dyn AsWidgetPod; 16]> {
        SmallVec::new()
    }

    fn children_mut(&mut self) -> SmallVec<[&mut dyn AsWidgetPod; 16]> {
        SmallVec::new()
    }
}

impl Widget for RawLabel {
    #[instrument(name = "RawLabel", level = "trace", skip(self, ctx, event, _env))]
    fn on_event(&mut self, ctx: &mut EventCtx, event: &Event, _env: &Env) {
        match event {
            Event::MouseUp(event) => {
                // Account for the padding
                let pos = event.pos - Vec2::new(LABEL_X_PADDING, 0.0);
                if let Some(link) = self.layout.link_for_pos(pos) {
                    todo!();
                    //ctx.submit_command(link.command.clone());
                }
            }
            Event::MouseMove(event) => {
                // Account for the padding
                let pos = event.pos - Vec2::new(LABEL_X_PADDING, 0.0);

                if self.layout.link_for_pos(pos).is_some() {
                    ctx.set_cursor(&Cursor::Pointer);
                } else {
                    ctx.clear_cursor();
                }
            }
            _ => {}
        }
    }

    #[instrument(name = "RawLabel", level = "trace", skip(self, ctx, event, _env))]
    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, _env: &Env) {
        match event {
            LifeCycle::DisabledChanged(disabled) => {
                let color = if *disabled {
                    KeyOrValue::Key(crate::theme::DISABLED_TEXT_COLOR)
                } else {
                    self.default_text_color.clone()
                };
                self.layout.set_text_color(color);
                ctx.request_layout();
            }
            _ => {}
        }
    }

    #[instrument(name = "RawLabel", level = "trace", skip(self, ctx, bc, env))]
    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, env: &Env) -> Size {
        bc.debug_check("Label");

        let width = match self.line_break_mode {
            LineBreaking::WordWrap => bc.max().width - LABEL_X_PADDING * 2.0,
            _ => f64::INFINITY,
        };

        self.layout.set_wrap_width(width);
        self.layout.rebuild_if_needed(ctx.text(), env);

        let text_metrics = self.layout.layout_metrics();
        ctx.set_baseline_offset(text_metrics.size.height - text_metrics.first_baseline);
        let size = bc.constrain(Size::new(
            text_metrics.size.width + 2. * LABEL_X_PADDING,
            text_metrics.size.height,
        ));
        trace!("Computed size: {}", size);
        size
    }

    #[instrument(name = "RawLabel", level = "trace", skip(self, ctx, _env))]
    fn paint(&mut self, ctx: &mut PaintCtx, _env: &Env) {
        let origin = Point::new(LABEL_X_PADDING, 0.0);
        let label_size = ctx.size();

        if self.line_break_mode == LineBreaking::Clip {
            ctx.clip(label_size.to_rect());
        }
        self.draw_at(ctx, origin)
    }

    fn children(&self) -> SmallVec<[&dyn AsWidgetPod; 16]> {
        SmallVec::new()
    }

    fn children_mut(&mut self) -> SmallVec<[&mut dyn AsWidgetPod; 16]> {
        SmallVec::new()
    }
}

impl Default for RawLabel {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Label {
    type Target = RawLabel;
    fn deref(&self) -> &Self::Target {
        &self.label
    }
}

impl DerefMut for Label {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.label
    }
}
impl From<String> for LabelText {
    fn from(src: String) -> LabelText {
        LabelText::Static(Static::new(src.into()))
    }
}

impl From<&str> for LabelText {
    fn from(src: &str) -> LabelText {
        LabelText::Static(Static::new(src.into()))
    }
}

impl From<ArcStr> for LabelText {
    fn from(string: ArcStr) -> LabelText {
        LabelText::Static(Static::new(string))
    }
}

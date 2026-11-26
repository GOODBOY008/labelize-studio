use super::*;
use labelize_printer_core::design::ObjectStyle;

impl Studio {
    fn style_choices<T: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        selected: T,
        options: impl IntoIterator<Item = (T, &'static str)>,
        change: fn(&mut ObjectStyle, T),
        cx: &mut Context<Self>,
    ) -> Div {
        row().children(options.into_iter().enumerate().map(|(i, (value, title))| {
            Button::new((id, i))
                .small()
                .label(title)
                .selected(selected == value)
                .disabled(self.selected_object().is_some_and(|o| o.style.locked))
                .on_click(cx.listener(move |s, _, w, cx| {
                    s.change_style(w, cx, |style| change(style, value))
                }))
        }))
    }
    pub(super) fn object_properties(&self, cx: &mut Context<Self>) -> Div {
        let Some(o) = self.selected_object() else {
            return div();
        };
        let style = &o.style;
        let mut content = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(section(self.t(Text::ObjectProperties)))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.object_name(o.kind)),
            )
            .child(self.field(9, self.t(Text::Name)))
            .child(
                row()
                    .child(
                        Button::new("object-visible")
                            .small()
                            .icon(if style.visible {
                                IconName::Eye
                            } else {
                                IconName::EyeOff
                            })
                            .label(if style.visible {
                                self.t(Text::Visible)
                            } else {
                                self.t(Text::Hidden)
                            })
                            .on_click(cx.listener(|s, _, w, cx| {
                                if let Some(i) = s.selected {
                                    s.layer_flag(i, false, w, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("object-lock")
                            .small()
                            .selected(style.locked)
                            .label(if style.locked {
                                self.t(Text::Unlock)
                            } else {
                                self.t(Text::Lock)
                            })
                            .on_click(cx.listener(|s, _, w, cx| {
                                if let Some(i) = s.selected {
                                    s.layer_flag(i, true, w, cx);
                                }
                            })),
                    ),
            )
            .when(style.locked, |d| {
                d.child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(self.t(Text::UnlockHelp)),
                )
            })
            .child(
                field_row()
                    .child(self.field(2, "X · mm"))
                    .child(self.field(3, "Y · mm")),
            )
            .child(
                field_row()
                    .child(self.field(4, self.t(Text::WidthMm)))
                    .child(self.field(5, self.t(Text::HeightMm))),
            )
            .child(section(self.t(Text::Rotation)))
            .child(row().children([0, 90, 180, 270].into_iter().map(|angle| {
                Button::new(("object-rotation", angle as usize))
                    .small()
                    .label(format!("{angle}°"))
                    .selected(style.rotation == angle)
                    .disabled(style.locked)
                    .on_click(cx.listener(move |s, _, w, cx| {
                        let result = s.edit_design(w, cx, |d, i| {
                            d.rotate(i, angle)?;
                            Ok(i)
                        });
                        if let Err(e) = result {
                            s.fail(e, cx);
                        }
                    }))
            })));
        if o.kind.has_content() {
            content = content
                .child(
                    Button::new("bind-variable")
                        .small()
                        .selected(o.variable)
                        .disabled(style.locked)
                        .label(self.t(Text::BindVariables))
                        .on_click(cx.listener(|s, _, w, cx| {
                            if let Err(e) = s.edit_design(w, cx, |d, i| {
                                d.set_variable(i, !d.objects[i].variable)?;
                                Ok(i)
                            }) {
                                s.fail(format!("{e:#}"), cx);
                            }
                        })),
                )
                .child(self.field(
                    8,
                    if o.variable {
                        self.t(Text::TemplateContent)
                    } else {
                        self.t(Text::Content)
                    },
                ));
            if o.variable {
                content = content.child(section(self.t(Text::SampleValues)));
                for (name, input) in &self.sample_inputs {
                    content = content.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xs().child(name.clone()))
                            .child(Input::new(input).disabled(self.loading)),
                    );
                }
            }
        }
        if o.kind == ObjectKind::Text
            || (o.kind.is_barcode() && style.human_readable && !o.kind.is_matrix())
        {
            content = content
                .child(self.field(6, self.t(Text::FontSize)))
                .child(section(self.t(Text::Font)))
                .child(self.style_choices(
                    "object-font",
                    style.font,
                    [
                        (TextFont::Document, self.t(Text::DocumentFont)),
                        (TextFont::Monospace, self.t(Text::MonoFont)),
                        (TextFont::BoldMonospace, self.t(Text::MonoBoldFont)),
                        (TextFont::CondensedBold, self.t(Text::CondensedBoldFont)),
                    ],
                    |s, v| s.font = v,
                    cx,
                ));
        }
        if o.kind == ObjectKind::Text {
            content = content
                .child(self.style_choices(
                    "text-wrap",
                    style.wrap,
                    [
                        (false, self.t(Text::ManualBreaks)),
                        (true, self.t(Text::WrapText)),
                    ],
                    |s, v| s.wrap = v,
                    cx,
                ))
                .child(self.field(10, self.t(Text::LineSpacing)));
        }
        if o.kind.is_shape() {
            content = content.child(self.field(7, self.t(Text::Stroke)));
            if o.kind != ObjectKind::Line {
                content = content.child(self.style_choices(
                    "shape-filled",
                    style.filled,
                    [(false, self.t(Text::Outline)), (true, self.t(Text::Filled))],
                    |s, v| s.filled = v,
                    cx,
                ));
            }
            if o.kind == ObjectKind::Rectangle {
                content = content.child(self.field(11, self.t(Text::CornerRadius)));
            }
            if o.kind == ObjectKind::Line {
                content = content.child(self.style_choices(
                    "line-direction",
                    style.line_direction,
                    [
                        (LineDirection::Horizontal, self.t(Text::Horizontal)),
                        (LineDirection::Vertical, self.t(Text::Vertical)),
                        (LineDirection::Down, "↘"),
                        (LineDirection::Up, "↗"),
                    ],
                    |s, v| s.line_direction = v,
                    cx,
                ));
            }
        }
        if o.kind == ObjectKind::Image {
            content = content
                .child(self.style_choices(
                    "image-fit",
                    style.image_fit,
                    [
                        (ImageFit::Contain, self.t(Text::Contain)),
                        (ImageFit::Cover, self.t(Text::Cover)),
                        (ImageFit::Stretch, self.t(Text::Stretch)),
                    ],
                    |s, v| s.image_fit = v,
                    cx,
                ))
                .child(self.field(16, self.t(Text::Threshold)))
                .child(
                    Button::new("replace-image")
                        .small()
                        .label(self.t(Text::ReplaceImage))
                        .disabled(style.locked || self.loading)
                        .on_click(cx.listener(|s, _, w, cx| s.pick_file(3, w, cx))),
                );
        }
        if o.kind.is_barcode() {
            content = content
                .child(section(self.t(Text::BarcodeSettings)))
                .child(self.field(12, self.t(Text::ModuleWidth)))
                .child(self.field(13, self.t(Text::QuietZone)))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(self.language.format(
                            Text::QuietZoneMinimum,
                            &[("modules", &o.kind.minimum_quiet().to_string())],
                        )),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(self.t(Text::WholeDots)),
                );
            if !o.kind.is_matrix() {
                content = content.child(self.style_choices(
                    "barcode-caption",
                    style.human_readable,
                    [
                        (false, self.t(Text::HideCaption)),
                        (true, self.t(Text::ShowCaption)),
                    ],
                    |s, v| s.human_readable = v,
                    cx,
                ));
            }
            match o.kind {
                ObjectKind::Qr => {
                    content = content.child(section(self.t(Text::ErrorCorrection))).child(
                        self.style_choices(
                            "qr-ecc",
                            style.qr_ecc,
                            [
                                (QrEcc::L, "L"),
                                (QrEcc::M, "M"),
                                (QrEcc::Q, "Q"),
                                (QrEcc::H, "H"),
                            ],
                            |s, v| s.qr_ecc = v,
                            cx,
                        ),
                    );
                }
                ObjectKind::DataMatrix => {
                    content = content.child(self.style_choices(
                        "matrix-shape",
                        style.matrix_shape,
                        [
                            (MatrixShape::Square, self.t(Text::Square)),
                            (MatrixShape::Rectangle, self.t(Text::Rectangular)),
                        ],
                        |s, v| s.matrix_shape = v,
                        cx,
                    ));
                }
                ObjectKind::Pdf417 => {
                    content = content
                        .child(self.field(14, self.t(Text::PdfColumns)))
                        .child(self.field(15, self.t(Text::PdfCorrection)));
                }
                ObjectKind::Ean13 | ObjectKind::Ean8 | ObjectKind::Upca => {
                    content = content.child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(self.t(Text::OptionalCheckDigit)),
                    );
                }
                ObjectKind::Itf => {
                    content = content.child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(self.t(Text::EvenDigits)),
                    );
                }
                _ => {}
            }
        }
        if o.kind == ObjectKind::Text || o.kind == ObjectKind::Image || o.kind.is_barcode() {
            content = content
                .child(section(self.t(Text::ContentAlignment)))
                .child(self.style_choices(
                    "content-horizontal",
                    style.horizontal,
                    [
                        (HorizontalAlign::Left, self.t(Text::Left)),
                        (HorizontalAlign::Center, self.t(Text::Center)),
                        (HorizontalAlign::Right, self.t(Text::Right)),
                    ],
                    |s, v| s.horizontal = v,
                    cx,
                ));
            if !o.kind.is_barcode() || o.kind.is_matrix() {
                content = content.child(self.style_choices(
                    "content-vertical",
                    style.vertical,
                    [
                        (VerticalAlign::Top, self.t(Text::Top)),
                        (VerticalAlign::Middle, self.t(Text::Middle)),
                        (VerticalAlign::Bottom, self.t(Text::Bottom)),
                    ],
                    |s, v| s.vertical = v,
                    cx,
                ));
            }
        }
        content = content
            .child(section(self.t(Text::Background)))
            .child(self.style_choices(
                "object-background",
                style.background,
                [
                    (Background::Transparent, self.t(Text::Transparent)),
                    (Background::White, self.t(Text::White)),
                    (Background::Black, self.t(Text::Black)),
                ],
                |s, v| s.background = v,
                cx,
            ))
            .child(section(self.t(Text::AlignToLabel)))
            .child(
                row().children(
                    [
                        (Alignment::Left, self.t(Text::Left)),
                        (Alignment::Center, self.t(Text::HorizontalCenter)),
                        (Alignment::Right, self.t(Text::Right)),
                        (Alignment::Top, self.t(Text::Top)),
                        (Alignment::Middle, self.t(Text::VerticalCenter)),
                        (Alignment::Bottom, self.t(Text::Bottom)),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (alignment, title))| {
                        Button::new(("align-object", i))
                            .small()
                            .label(title)
                            .disabled(style.locked)
                            .on_click(
                                cx.listener(move |s, _, w, cx| s.align_object(alignment, w, cx)),
                            )
                    }),
                ),
            )
            .child(section(self.t(Text::LayerOrder)))
            .child(
                row().children(
                    [
                        (LayerOrder::Front, self.t(Text::BringToFront)),
                        (LayerOrder::Raise, self.t(Text::MoveForward)),
                        (LayerOrder::Lower, self.t(Text::MoveBackward)),
                        (LayerOrder::Back, self.t(Text::SendToBack)),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (order, title))| {
                        Button::new(("layer-order", i))
                            .small()
                            .label(title)
                            .disabled(style.locked)
                            .on_click(cx.listener(move |s, _, w, cx| s.reorder_layer(order, w, cx)))
                    }),
                ),
            )
            .child(
                row()
                    .child(
                        Button::new("duplicate-object")
                            .small()
                            .icon(IconName::Copy)
                            .label(self.t(Text::Duplicate))
                            .on_click(cx.listener(|s, _, w, cx| s.duplicate(w, cx))),
                    )
                    .child(
                        Button::new("delete-object")
                            .small()
                            .ghost()
                            .icon(IconName::Delete)
                            .label(self.t(Text::Delete))
                            .disabled(style.locked)
                            .on_click(cx.listener(|s, _, w, cx| s.delete(w, cx))),
                    ),
            );
        content
    }
}

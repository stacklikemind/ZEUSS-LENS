mod model;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    Disableable, Theme, ThemeMode,
};
use gpui_kit::{prelude::*, *};
use serde_json::{json, Value};
#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;
use std::sync::Arc;
gpui_kit::assets::icon_assets!(AppAssets, [Check, ChevronDown, ChevronUp, Close]);

actions!(forge, [Quit]);
struct Studio {
    inputs: Vec<Entity<InputState>>,
    present: [bool; 2],
    exact: bool,
    fixed: bool,
    lens_type: u8,
    profile: Option<Value>,
    approximate: bool,
    output: Option<model::Output>,
    preview: Option<Arc<Image>>,
    coffee: Arc<Image>,
    status: String,
    subscriptions: Vec<Subscription>,
}
impl Studio {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let inputs: Vec<_> = ["0", "0", "0", "0", "0", "0", "0", "0", ""]
            .iter()
            .map(|v| cx.new(|cx| InputState::new(window, cx).default_value(*v)))
            .collect();
        let mut this = Self {
            inputs,
            present: [true, true],
            exact: false,
            fixed: false,
            lens_type: 3,
            profile: None,
            approximate: false,
            output: None,
            preview: None,
            coffee: Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../assets/buy-me-a-coffee.png").to_vec(),
            )),
            status: "Ready. All processing stays on this device.".into(),
            subscriptions: vec![],
        };
        for input in &this.inputs {
            this.subscriptions
                .push(cx.subscribe(input, |this, _, event, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.invalidate(cx);
                    }
                }));
        }
        this.generate(cx);
        this
    }
    fn invalidate(&mut self, cx: &mut Context<Self>) {
        self.approximate = false;
        self.profile = None;
        self.output = None;
        self.preview = None;
        self.generate(cx);
    }
    fn request(&self, cx: &App) -> Result<Value, String> {
        let mut eyes = vec![];
        for side in 0..2 {
            eyes.push(if self.present[side] {
                model::entered_eye(
                    &(side * 4..side * 4 + 4)
                        .map(|i| self.inputs[i].read(cx).value().to_string())
                        .collect::<Vec<_>>(),
                )
                .map_err(|message| {
                    format!(
                        "{}: {message}",
                        if side == 0 { "Right eye" } else { "Left eye" }
                    )
                })?
            } else {
                Value::Null
            });
        }
        Ok(
            json!({"op":"generate","right":eyes[0],"left":eyes[1],"axis_policy":if self.exact {"exact"} else {"native"},"lens_type":self.lens_type}),
        )
    }
    fn generate(&mut self, cx: &mut Context<Self>) {
        let fixed = self
            .fixed
            .then(|| self.inputs[8].read(cx).value().trim().to_string());
        self.output = None;
        self.preview = None;
        let generation = self.request(cx).and_then(|mut request| {
            let profile = model::review(&request)?;
            let can_generate = profile["can_generate"] == true;
            let needs_approximation = profile["needs_approximation"] == true;
            self.profile = Some(profile);
            if !can_generate {
                return Err("Check the prescription values in the code profile below.".into());
            }
            if needs_approximation && !self.approximate {
                return Err("Your full prescription does not fit the code table. Review the optional approximate profile before trying it.".into());
            }
            request["diopter_policy"] = json!(if self.approximate {"reduce_cylinder"} else {"exact"});
            model::generate(request, fixed)
        });
        match generation {
            Ok(output) => {
                self.preview = Some(Arc::new(Image::from_bytes(
                    ImageFormat::Png,
                    output.png.clone(),
                )));
                self.status = format!(
                    "{} · generated locally · {} identity attempt(s)",
                    if self.approximate {
                        "Approximate profile"
                    } else {
                        "Exact sphere / cylinder"
                    },
                    output.attempts
                );
                self.output = Some(output);
            }
            Err(error) => self.status = error,
        }
        cx.notify();
    }
    fn profile_panel(&self, cx: &mut Context<Self>) -> Div {
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .rounded_lg()
            .bg(rgb(0x17251f));
        panel = panel.child(div().text_lg().child("Code profile"));
        let Some(profile) = &self.profile else {
            return panel.child("Enter complete prescription values to review a profile.");
        };
        let approximate = profile["needs_approximation"] == true;
        panel = panel.child(div().text_sm().text_color(rgb(if approximate {0xffcf87} else {0xb1c6b9})).child(
            if approximate {"Optional approximation: keep your sphere and reduce cylinder magnitude to the nearest encodable value. Tracking benefit is untested."}
            else {"Your insert prescription stays as entered. Code values and nominal calibration are shown separately."}));
        for (side, label) in [("right", "Right / OD"), ("left", "Left / OS")] {
            let eye = &profile["eyes"][side];
            let mut row = div().flex().flex_col().gap_1().text_sm().child(label);
            if let Some(error) = eye["error"].as_str() {
                row = row.child(div().text_color(rgb(0xffcf87)).child(error.to_string()));
            } else if eye["entered"].is_null() {
                row = row.child("No lens present");
            } else {
                for (field, label) in [("entered", "Your inserts"), ("encoded", "In the code")] {
                    let values = &eye[field];
                    row = row.child(format!(
                        "{label}: {:+.2} / {:+.2} / {}°",
                        values["sphere"].as_f64().unwrap_or(0.0),
                        values["cylinder"].as_f64().unwrap_or(0.0),
                        values["axis"]
                    ));
                }
                let calibration = &eye["calibration"];
                row = row.child(div().text_color(rgb(0xb1c6b9)).child(
                    if calibration["available"] == true {
                        format!(
                            "Nominal: {:+.2} / {:+.2} D (minus cylinder)",
                            calibration["sphere"].as_f64().unwrap_or(0.0),
                            calibration["cylinder"].as_f64().unwrap_or(0.0)
                        )
                    } else {
                        "Nominal calibration unavailable for this pair".into()
                    },
                ));
            }
            panel = panel.child(row);
        }
        if approximate && profile["can_generate"] == true {
            panel = panel.child(div().text_sm().child("This changes the code only. Keep inserts made for your actual prescription. Editing any input clears this selection."))
                .child(Button::new("approximate-profile").label(if self.approximate {"Stop using approximate profile"} else {"Try approximate profile"})
                    .on_click(cx.listener(|this,_,_,cx|{this.approximate=!this.approximate;this.generate(cx);})));
        }
        panel
    }
    #[cfg(not(target_family = "wasm"))]
    fn export(&mut self, extension: &'static str, cx: &mut Context<Self>) {
        let Some(output) = &self.output else { return };
        let bytes = match extension {
            "svg" => output.svg.as_bytes().to_vec(),
            "png" => output.png.clone(),
            _ => match serde_json::to_vec_pretty(&output.report) {
                Ok(v) => v,
                Err(e) => {
                    self.status = e.to_string();
                    cx.notify();
                    return;
                }
            },
        };
        let dir = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let dialog = cx.prompt_for_new_path(&dir, Some(&format!("zeuss-lens.{extension}")));
        cx.spawn(async move |this, cx| {
            let status = match dialog.await {
                Ok(Ok(Some(path))) => match std::fs::write(&path, &bytes) {
                    Ok(()) => format!("Saved {}", path.display()),
                    Err(e) => format!("Could not save file: {e}"),
                },
                Ok(Ok(None)) => "Export cancelled.".into(),
                Ok(Err(e)) => format!("Could not open save dialog: {e}"),
                Err(e) => format!("Save dialog closed: {e}"),
            };
            let _ = this.update(cx, |this, cx| {
                this.status = status;
                cx.notify();
            });
        })
        .detach();
    }
    fn eye_panel(&self, side: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let label = if side == 0 {
            "Right eye · OD"
        } else {
            "Left eye · OS"
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_lg()
            .bg(rgb(0x17251f))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(label)
                    .child(
                        Button::new(("present", side))
                            .label(if self.present[side] {
                                "Present"
                            } else {
                                "Absent"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.present[side] = !this.present[side];
                                this.invalidate(cx);
                            })),
                    ),
            )
            .child(
                div().flex().gap_2().children(
                    ["Sphere / D", "Cylinder / D", "Axis / °", "VR add / D"]
                        .iter()
                        .enumerate()
                        .map(|(field, label)| {
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .child(*label)
                                .child(div().text_xs().text_color(rgb(0xb1c6b9)).child(
                                    model::parameter_hint(
                                        field,
                                        &self.inputs[side * 4].read(cx).value(),
                                        &self.inputs[side * 4 + 1].read(cx).value(),
                                    ),
                                ))
                                .child(
                                    Input::new(&self.inputs[side * 4 + field])
                                        .disabled(!self.present[side]),
                                )
                        }),
                ),
            )
    }
    fn report(&self) -> Div {
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .rounded_lg()
            .bg(rgb(0x17251f));
        panel = panel.child(div().text_lg().child("Encoded values & checks"));
        if let Some(output) = &self.output {
            let result = &output.report["result"];
            for (side, label) in [("right", "Right / OD"), ("left", "Left / OS")] {
                let e = &result["encoded"][side];
                let line = if e.is_null() {
                    format!("{label}: absent")
                } else {
                    format!(
                        "{label}: sphere {} D · cylinder {} D · axis {}° · VR add {} D",
                        e["sphere"],
                        e["cylinder"],
                        e["axis"],
                        e["rx_offset_id"].as_u64().unwrap_or(0) as f64 / 4.0
                    )
                };
                panel = panel.child(line);
                let calibration = &result["calibration"][side];
                let text = if e.is_null() {
                    "No lens present".into()
                } else if calibration["available"] == false {
                    "Calibration limits unavailable for this pair".into()
                } else {
                    format!(
                        "Calibration: sphere {} D / cylinder {} D{}",
                        calibration["sphere"],
                        calibration["cylinder"],
                        if calibration["clamped"] == true {
                            " · clamped to calibration limits"
                        } else if calibration["changed"] == true {
                            " · adjusted"
                        } else {
                            " · unchanged"
                        }
                    )
                };
                panel = panel.child(div().text_sm().text_color(rgb(0xb1c6b9)).child(text));
            }
            if let Some(warnings) = result["warnings"].as_array() {
                for warning in warnings {
                    panel = panel.child(
                        div()
                            .text_color(rgb(0xffcf87))
                            .child(warning.as_str().unwrap_or("").to_string()),
                    );
                }
            }
            let v = &result["validation"];
            let passed = v["payload_roundtrip"] == true
                && v["gap_checksum"] == true
                && v["arc_checksum"] == true
                && v["redundant_colors"] == true
                && ["metadata_syndromes", "gap_syndromes", "arc_syndromes"]
                    .iter()
                    .all(|k| {
                        v[*k]
                            .as_array()
                            .is_some_and(|a| a.len() == 2 && a.iter().all(|n| n == 0))
                    });
            panel = panel.child(div().text_sm().child(if passed {
                "Payload round trip · parity · checksums · redundant colors: passed"
            } else {
                "One or more symbol-bit checks did not pass. Inspect the JSON report."
            }));
        } else {
            panel = panel.child("Generate a code to inspect its encoded values.");
        }
        panel.child(div().text_sm().text_color(rgb(0xb1c6b9)).child("Calibration adjustments are separate calculations, not equivalent prescriptions or clinical recommendations. Headset enrollment and camera recognition remain unverified."))
    }
}
impl Render for Studio {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = window.viewport_size().width < px(950.);
        let image_size = if compact {
            (window.viewport_size().width - px(48.)).min(px(410.))
        } else {
            px(410.)
        };
        let mut preview = div()
            .flex()
            .items_center()
            .justify_center()
            .w_full()
            .h(image_size + px(10.))
            .rounded_lg()
            .bg(rgb(0xffffff));
        if let Some(image) = &self.preview {
            preview = preview.child(
                img(image.clone())
                    .size(image_size)
                    .object_fit(ObjectFit::Contain),
            );
        } else {
            preview = preview.child(
                div()
                    .text_color(rgb(0x52685b))
                    .child("Enter supported values to generate a code"),
            );
        }
        let disabled = self.output.is_none();
        div().id("studio-scroll").size_full().overflow_y_scroll().bg(rgb(0x0c1712)).text_color(rgb(0xe4eee7)).font_family("IBM Plex Sans").p_6()
            .child(div().flex().flex_col().gap_5()
                .child(div().flex().justify_between().items_center()
                    .child(div().text_2xl().child("ZEUSS LENS"))
                    .child(div().flex().gap_3().items_center().child(Button::new("coffee").h(px(60.)).p_0().child(img(self.coffee.clone()).w(px(217.)).h(px(60.)).object_fit(ObjectFit::Contain)).on_click(|_,_,cx|cx.open_url("https://www.buymeacoffee.com/stacklikemind")))))
                .child(div().text_color(rgb(0xb1c6b9)).child("Enter your physical insert prescription. Review the code profile; edit values to regenerate."))
                .child(div().flex().gap_5().items_start().when(compact, |d|d.flex_col())
                    .child(div().w(px(450.)).when(compact,|d|d.w_full()).flex_shrink_0().flex().flex_col().gap_3()
                        .child(self.eye_panel(0,cx)).child(self.eye_panel(1,cx))
                        .child(div().text_sm().child("Keep your actual prescription in these fields. Sphere / cylinder use 0.25 D steps. The bounds describe the exact code table; unsupported pairs may offer an optional approximation. This table is not an Apple or ZEISS prescription eligibility test."))
                        .child(self.profile_panel(cx))
                        .child(Button::new("axis-policy").label(if self.exact {"Axis: exact (reject rounding)"} else {"Axis: native (report rounding)"})
                            .on_click(cx.listener(|this,_,_,cx|{this.exact=!this.exact;this.invalidate(cx);})))
                        .child(Button::new("lens-type").label(format!("Lens type: {}",match self.lens_type {1=>"reader",2=>"developer",_=>"personal"}))
                            .on_click(cx.listener(|this,_,_,cx|{this.lens_type=this.lens_type%3+1;this.invalidate(cx);})))
                        .child(Button::new("identity-mode").label(if self.fixed {"Identity: fixed (20 hex characters)"} else {"Identity: fresh random bytes"})
                            .on_click(cx.listener(|this,_,_,cx|{this.fixed=!this.fixed;this.invalidate(cx);})))
                        .when(self.fixed,|this|this.child(Input::new(&self.inputs[8])))
                        .child(Button::new("generate").primary().label("Regenerate code").on_click(cx.listener(|this,_,_,cx|this.generate(cx)))))
                    .child(div().flex_1().min_w_0().when(compact,|d|d.w_full()).flex().flex_col().gap_3().child(preview)
                        .child(div().flex().gap_2().children(["svg","png","json"].into_iter().map(|ext| {
                            Button::new(ext).label(format!("Save {}",ext.to_uppercase())).disabled(disabled)
                                .on_click(cx.listener(move |this,_,_,cx|this.export(ext,cx)))
                        })))
                        .child(div().text_sm().text_color(rgb(0xb1c6b9)).child("Synthetic identities do not establish authenticity or factory calibration. No inputs are uploaded or saved automatically."))))
                .child(div().text_sm().text_color(rgb(0xb1c6b9)).child("To try a code: wear your inserts, scan during insert pairing, then complete eye setup. Check small targets in the center and near the edges. If tracking is worse, return to your previous setup. Scan acceptance alone does not establish tracking quality."))
                .child(div().p_3().rounded_md().bg(rgb(0x243c2e)).child(self.status.clone()))
                .child(self.report())
)
    }
}
#[cfg(not(target_family = "wasm"))]
pub fn run_native() {
    if std::env::args().any(|arg| arg == "--self-test") {
        match model::self_test() {
            Ok(()) => {
                println!("Embedded encoder, payload round trip and PNG export passed.");
                return;
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1)
            }
        }
    }
    gpui_kit::application().with_assets(AppAssets).run(launch);
}

fn launch(cx: &mut App) {
    cx.text_system()
        .add_fonts(vec![std::borrow::Cow::Borrowed(
            include_bytes!("../fonts/IBMPlexSans-Regular.ttf").as_slice(),
        )])
        .expect("Could not load embedded font");
    gpui_kit::init(cx);
    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |theme| {
        theme.font_family = "IBM Plex Sans".into();
        theme.mono_font_family = "IBM Plex Sans".into();
    });
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("ctrl-q", Quit, None),
    ]);
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
    let bounds = Bounds::centered(None, size(px(1080.), px(900.)), cx);
    gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(1000.), px(680.))),
            titlebar: Some(TitlebarOptions {
                title: Some("ZEUSS LENS".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
        |window, cx| cx.new(|cx| Studio::new(window, cx)),
    )
    .expect("Could not open desktop window");
    cx.activate(true);
    #[cfg(target_family = "wasm")]
    if let (Some(window), Ok(event)) = (web_sys::window(), web_sys::Event::new("optic-forge-ready"))
    {
        let _ = window.dispatch_event(&event);
    }
}

#[cfg(target_family = "wasm")]
thread_local! { static APPLICATION: std::cell::RefCell<Option<ApplicationHandle>> = const { std::cell::RefCell::new(None) }; }

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn run_web() {
    console_error_panic_hook::set_once();
    gpui_kit::platform::web_init();
    let platform = std::rc::Rc::new(
        gpui_kit::web::WebPlatform::new_with_backend_and_font_fallback(
            false,
            gpui_kit::web::WebBackendPreference::Auto,
            gpui_kit::web::CanvasFontFallback::EmojiAndCjk,
        ),
    );
    let app = Application::with_platform(platform).with_assets(AppAssets);
    APPLICATION.with(|slot| *slot.borrow_mut() = Some(app.run_embedded(launch)));
}

#[cfg(target_family = "wasm")]
impl Studio {
    fn export(&mut self, extension: &'static str, cx: &mut Context<Self>) {
        use wasm_bindgen::JsCast;
        let Some(output) = &self.output else {
            return;
        };
        let bytes = match extension {
            "svg" => output.svg.as_bytes().to_vec(),
            "png" => output.png.clone(),
            _ => serde_json::to_vec_pretty(&output.report).unwrap(),
        };
        let download = || -> Result<(), wasm_bindgen::JsValue> {
            let window = web_sys::window()
                .ok_or_else(|| wasm_bindgen::JsValue::from_str("No browser window"))?;
            let document = window
                .document()
                .ok_or_else(|| wasm_bindgen::JsValue::from_str("No document"))?;
            let parts = js_sys::Array::new();
            parts.push(&js_sys::Uint8Array::from(bytes.as_slice()));
            let options = web_sys::BlobPropertyBag::new();
            options.set_type(match extension {
                "svg" => "image/svg+xml",
                "png" => "image/png",
                _ => "application/json",
            });
            let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)?;
            let url = web_sys::Url::create_object_url_with_blob(&blob)?;
            let anchor = document
                .create_element("a")?
                .dyn_into::<web_sys::HtmlAnchorElement>()?;
            anchor.set_href(&url);
            anchor.set_download(&format!("zeuss-lens.{extension}"));
            document
                .body()
                .ok_or_else(|| wasm_bindgen::JsValue::from_str("No document body"))?
                .append_child(&anchor)?;
            anchor.click();
            anchor.remove();
            let cleanup = wasm_bindgen::closure::Closure::once_into_js(move || {
                let _ = web_sys::Url::revoke_object_url(&url);
            });
            window.set_timeout_with_callback_and_timeout_and_arguments_0(
                cleanup.unchecked_ref(),
                1000,
            )?;
            Ok(())
        };
        self.status = match download() {
            Ok(()) => format!("{} download requested.", extension.to_uppercase()),
            Err(e) => format!("Download failed: {e:?}"),
        };
        cx.notify();
    }
}

//! A data-driven sky gallery. The artwork is bundled locally; captions and
//! weather facts are filled from the same forecast shown on the home page.
use makepad_widgets::*;

use crate::model::{self, City, Forecast};

const SKY: &[u8] = include_bytes!("assets/sky-backdrop.png");
const SKY_PATH: &str = "weather-agent/gallery-sky.png";

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    let Caption = Label{
        width: Fill height: Fit padding: 0 margin: 0 max_lines: 3
        draw_text +: { color: #F5F1E9 text_style: theme.font_regular{font_size: 12.0 line_spacing: 1.5} }
    }
    let Muted = Label{
        width: Fill height: Fit padding: 0 margin: 0 max_lines: 2
        draw_text +: { color: #B6B2AA text_style: theme.font_regular{font_size: 10.0} }
    }
    let Fact = View{
        width: Fill height: Fit flow: Down spacing: 5
        padding: Inset{top: 10 left: 11 right: 11 bottom: 10}
        show_bg: true
        draw_bg +: { color: #292929 border_radius: 13.0 }
        key := Muted{}
        value := Label{ width: Fill height: Fit padding: 0 margin: 0
            draw_text +: { color: #F5F1E9 text_style: theme.font_bold{font_size: 13.0} } }
    }

    mod.widgets.SkyGalleryBase = #(SkyGallery::register_widget(vm))
    mod.widgets.SkyGallery = set_type_default() do mod.widgets.SkyGalleryBase{
        width: Fill height: Fill
        show_bg: true
        draw_bg +: { color: #191919 }
        ScrollYView{
            width: Fill height: Fill flow: Down spacing: 13
            padding: Inset{top: 16 left: 14 right: 14 bottom: 14}
            show_bg: true
            draw_bg +: { color: #191919 }
            scroll_bars +: { show_scroll_x: false }

            View{ width: Fill height: Fit flow: Down spacing: 4
                Label{ padding: 0 margin: 0 width: Fill height: Fit text: "今日天空"
                    draw_text +: { color: #F5F1E9 text_style: theme.font_bold{font_size: 21.0} } }
                Muted{ text: "一张跟着真实天气变化的城市天空" }
            }

            View{ width: Fill height: 250 flow: Overlay
                show_bg: true
                draw_bg +: { color: #242424 border_radius: 20.0 }
                artwork := Image{ width: Fill height: Fill fit: ImageFit.CropToFill }
                View{ width: Fill height: Fit flow: Down spacing: 5
                    padding: Inset{top: 16 left: 16 right: 16 bottom: 16}
                    align: Align{x: 0.0 y: 1.0}
                    show_bg: true
                    draw_bg +: { color: #101018AA border_radius: 14.0 }
                    art_condition := Label{ width: Fill height: Fit padding: 0 margin: 0
                        draw_text +: { color: #FFFFFF text_style: theme.font_bold{font_size: 18.0} } }
                    art_caption := Caption{}
                }
            }

            View{ width: Fill height: Fit flow: Right spacing: 8
                temp_fact := Fact{ key.text: "此刻气温" value.text: "—" }
                wind_fact := Fact{ key.text: "风况" value.text: "—" }
            }

            View{ width: Fill height: Fit flow: Down spacing: 8
                Label{ padding: 0 margin: 0 width: Fill height: Fit text: "天空日记"
                    draw_text +: { color: #F5F1E9 text_style: theme.font_bold{font_size: 13.0} } }
                Caption{ text: "这里会收藏每天的天气与天空。分享卡片和 AI 天空画可在下一轮接入生成服务。" }
            }
            Muted{ text: "天气数据：Open-Meteo · 天空素材：应用内置" }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct SkyGallery {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust(false)]
    loaded: bool,
}

impl SkyGallery {
    pub fn render(&mut self, cx: &mut Cx, city: &City, forecast: &Option<Forecast>) {
        if !self.loaded {
            self.loaded = true;
            let image = self.view.image(cx, ids!(artwork));
            let loaded = image.load_image_from_data_async(
                cx,
                std::path::Path::new(SKY_PATH),
                std::sync::Arc::new(SKY),
            ).is_ok();
            image.set_visible(cx, loaded);
        }

        let current = forecast.as_ref().and_then(|f| f.current.as_ref());
        let code = current.and_then(|c| c.weather_code).unwrap_or(-1);
        self.view.label(cx, ids!(art_condition)).set_text(cx, &format!("{} · {}", city.name, model::condition(code)));
        let caption = current.and_then(|c| c.temperature_2m).map(|t| {
            format!("此刻 {:.0}°，这幅天空以 {} 的天气为灵感。", t, model::condition(code))
        }).unwrap_or_else(|| "天气更新后，这里会出现与当地实况相配的天空记录。".into());
        self.view.label(cx, ids!(art_caption)).set_text(cx, &caption);

        let temp = current.and_then(|c| c.temperature_2m).map(|v| format!("{v:.0}°")).unwrap_or_else(|| "—".into());
        self.view.widget(cx, ids!(temp_fact)).label(cx, ids!(value)).set_text(cx, &temp);
        let wind = current.and_then(|c| c.wind_speed_10m).map(|v| format!("{v:.0} km/h")).unwrap_or_else(|| "—".into());
        self.view.widget(cx, ids!(wind_fact)).label(cx, ids!(value)).set_text(cx, &wind);
        self.view.redraw(cx);
    }
}

impl Widget for SkyGallery {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

// 浅色调配色和几个 label 刷新的小工具。
//
// script_mod 里写的是字面量 hex（声明期就定死），Rust 里改文字和颜色时用这里
// 的 vec4 常量，两边是同一套色。
use makepad_widgets::*;

// 颜色以 hex 记，取值时转成 makepad 的 0..1 float。
// 例：#3D7BFA = (61, 123, 250) = (0.235, 0.482, 0.980)。

// 纸面与卡片
pub const PAPER: Vec4f = Vec4f { x: 0.961, y: 0.969, z: 0.980, w: 1.0 }; // #F5F7FA
pub const CARD: Vec4f = Vec4f { x: 1.0, y: 1.0, z: 1.0, w: 1.0 };        // #FFFFFF
pub const HAIR: Vec4f = Vec4f { x: 0.902, y: 0.918, z: 0.941, w: 1.0 }; // #E6EAF0

// 文字四档，从深到浅
pub const INK: Vec4f = Vec4f { x: 0.106, y: 0.122, z: 0.153, w: 1.0 };   // #1B1F27
pub const INK_2: Vec4f = Vec4f { x: 0.357, y: 0.392, z: 0.447, w: 1.0 }; // #5B6472
pub const INK_3: Vec4f = Vec4f { x: 0.545, y: 0.584, z: 0.651, w: 1.0 }; // #8B95A6
pub const INK_4: Vec4f = Vec4f { x: 0.604, y: 0.639, z: 0.698, w: 1.0 }; // #9AA3B2

// 主色：一档蓝，选中、强调、用户气泡
pub const ACCENT: Vec4f = Vec4f { x: 0.235, y: 0.482, z: 0.980, w: 1.0 };     // #3D7BFA
pub const ACCENT_INK: Vec4f = Vec4f { x: 0.118, y: 0.345, z: 0.851, w: 1.0 }; // #1E58D9
pub const ACCENT_PAPER: Vec4f = Vec4f { x: 0.918, y: 0.945, z: 0.996, w: 1.0 }; // #EAF1FE

// 成功 / 错误
pub const OK: Vec4f = Vec4f { x: 0.204, y: 0.655, z: 0.447, w: 1.0 }; // #35A772
pub const ERR: Vec4f = Vec4f { x: 0.851, y: 0.322, z: 0.318, w: 1.0 }; // #D95251

/// 只改文字，颜色不动。
pub fn text(l: &LabelRef, cx: &mut Cx, value: &str) {
    l.set_text(cx, value);
}

/// 只改文字颜色。
pub fn tint(l: &LabelRef, cx: &mut Cx, color: Vec4f) {
    l.set_text_color(cx, color);
}

/// 一次改文字和颜色。
pub fn set_label(l: &LabelRef, cx: &mut Cx, value: &str, color: Vec4f) {
    l.set_text(cx, value);
    l.set_text_color(cx, color);
}

/// 显隐一个部件。
pub fn show(w: &WidgetRef, cx: &mut Cx, on: bool) {
    w.set_visible(cx, on);
}

/// 改一整个 draw_bg 的填充色（卡片、气泡这类块状背景）。
///
/// `script_apply_eval!` 走 `&mut self`，所以要先把 WidgetRef（一份 Rc）复制成
/// 可变的局部变量再交给宏；WidgetRef 本身不可 Copy。
pub fn fill(w: &WidgetRef, cx: &mut Cx, color: Vec4f) {
    let mut t = w.clone();
    makepad_widgets::script_apply_eval!(cx, t, { draw_bg.color: #(color) });
}

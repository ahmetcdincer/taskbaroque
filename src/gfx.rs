use std::cell::RefCell;
use std::collections::HashMap;

use windows::core::{w, Result, PCWSTR};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: a as f32 / 255.0 }
    }

    pub fn parse(s: &str) -> Option<Color> {
        let h = s.trim().trim_start_matches('#');
        let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
        match h.len() {
            6 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, 255)),
            8 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    pub fn alpha(self, a: f32) -> Color {
        Color { a: self.a * a, ..self }
    }

    fn d2d(self) -> D2D1_COLOR_F {
        D2D1_COLOR_F { r: self.r, g: self.g, b: self.b, a: self.a }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
    pub fn inset(&self, d: f32) -> Rect {
        Rect::new(self.x + d, self.y + d, self.w - 2.0 * d, self.h - 2.0 * d)
    }
    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F { left: self.x, top: self.y, right: self.x + self.w, bottom: self.y + self.h }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    Left,
    Center,
    Right,
}

struct Factories {
    d2d: ID2D1Factory,
    dw: IDWriteFactory,
    wic: IWICImagingFactory,
    icon_font: Vec<u16>,
}

thread_local! {
    static FACTORIES: RefCell<Option<Factories>> = const { RefCell::new(None) };
}

fn with_f<R>(f: impl FnOnce(&Factories) -> R) -> R {
    FACTORIES.with(|c| f(c.borrow().as_ref().expect("gfx::init çağrılmadı")))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn init() -> Result<()> {
    unsafe {
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;

        let mut coll = None;
        dw.GetSystemFontCollection(&mut coll, false)?;
        let mut icon_font = "Segoe MDL2 Assets";
        if let Some(coll) = coll {
            let (mut idx, mut exists) = (0u32, BOOL(0));
            if coll.FindFamilyName(w!("Segoe Fluent Icons"), &mut idx, &mut exists).is_ok() && exists.as_bool() {
                icon_font = "Segoe Fluent Icons";
            }
        }

        FACTORIES.with(|c| *c.borrow_mut() = Some(Factories { d2d, dw, wic, icon_font: wide(icon_font) }));
    }
    Ok(())
}

type FormatKey = (u32, i32, Align, bool);

pub struct Surface {
    pub w_px: i32,
    pub h_px: i32,
    pub scale: f32,
    memdc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    rt: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    font: Vec<u16>,
    formats: RefCell<HashMap<FormatKey, IDWriteTextFormat>>,
}

impl Surface {
    pub fn new(w: f32, h: f32, scale: f32, font: &str) -> Result<Surface> {
        let w_px = (w * scale).ceil() as i32;
        let h_px = (h * scale).ceil() as i32;
        unsafe {
            let (memdc, bmp, old) = create_dib(w_px, h_px)?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 0.0,
                dpiY: 0.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let rt = with_f(|f| f.d2d.CreateDCRenderTarget(&props))?;
            rt.SetDpi(96.0 * scale, 96.0 * scale);
            rt.BindDC(memdc, &RECT { left: 0, top: 0, right: w_px, bottom: h_px })?;
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = rt.CreateSolidColorBrush(&Color::rgba(255, 255, 255, 255).d2d(), None)?;
            Ok(Surface {
                w_px,
                h_px,
                scale,
                memdc,
                bmp,
                old,
                rt,
                brush,
                font: wide(font),
                formats: RefCell::new(HashMap::new()),
            })
        }
    }

    pub fn begin(&self) {
        unsafe {
            self.rt.BeginDraw();
            self.rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
        }
    }

    pub fn end(&self) {
        unsafe {
            let _ = self.rt.EndDraw(None, None);
        }
    }

    pub fn present(&self, hwnd: HWND, x: i32, y: i32) {
        unsafe {
            let dst = POINT { x, y };
            let size = SIZE { cx: self.w_px, cy: self.h_px };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                hwnd,
                None,
                Some(&dst),
                Some(&size),
                self.memdc,
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }

    pub fn push_clip(&self, r: Rect) {
        unsafe { self.rt.PushAxisAlignedClip(&r.d2d(), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) }
    }

    pub fn pop_clip(&self) {
        unsafe { self.rt.PopAxisAlignedClip() }
    }

    pub fn blit(&self, dc: HDC) {
        unsafe {
            let _ = BitBlt(dc, 0, 0, self.w_px, self.h_px, self.memdc, 0, 0, SRCCOPY);
        }
    }

    fn set_color(&self, c: Color) {
        unsafe { self.brush.SetColor(&c.d2d()) }
    }

    pub fn fill_round(&self, r: Rect, radius: f32, c: Color) {
        self.set_color(c);
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
        unsafe { self.rt.FillRoundedRectangle(&rr, &self.brush) }
    }

    pub fn stroke_round(&self, r: Rect, radius: f32, width: f32, c: Color) {
        self.set_color(c);
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
        unsafe { self.rt.DrawRoundedRectangle(&rr, &self.brush, width, None) }
    }

    fn stops(&self, colors: &[Color]) -> Option<ID2D1GradientStopCollection> {
        let n = colors.len().max(2) - 1;
        let stops: Vec<D2D1_GRADIENT_STOP> = colors
            .iter()
            .enumerate()
            .map(|(i, c)| D2D1_GRADIENT_STOP { position: i as f32 / n as f32, color: c.d2d() })
            .collect();
        unsafe { self.rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP).ok() }
    }

    fn linear(&self, colors: &[Color], r: Rect, vertical: bool) -> Option<ID2D1LinearGradientBrush> {
        let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: pt(r.x, r.y),
            endPoint: if vertical { pt(r.x, r.y + r.h) } else { pt(r.x + r.w, r.y) },
        };
        let stops = self.stops(colors)?;
        unsafe { self.rt.CreateLinearGradientBrush(&props, None, &stops).ok() }
    }

    pub fn fill_round_grad(&self, r: Rect, radius: f32, colors: &[Color], vertical: bool) {
        match colors {
            [] => {}
            [c] => self.fill_round(r, radius, *c),
            _ => {
                let Some(brush) = self.linear(colors, r, vertical) else { return };
                let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
                unsafe { self.rt.FillRoundedRectangle(&rr, &brush) }
            }
        }
    }

    pub fn stroke_round_grad(&self, r: Rect, radius: f32, width: f32, colors: &[Color], vertical: bool) {
        match colors {
            [] => {}
            [c] => self.stroke_round(r, radius, width, *c),
            _ => {
                let Some(brush) = self.linear(colors, r, vertical) else { return };
                let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
                unsafe { self.rt.DrawRoundedRectangle(&rr, &brush, width, None) }
            }
        }
    }

    pub fn fill_round_radial(&self, r: Rect, radius: f32, cx: f32, cy: f32, rx: f32, ry: f32, c: Color) {
        let props = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
            center: pt(cx, cy),
            gradientOriginOffset: pt(0.0, 0.0),
            radiusX: rx,
            radiusY: ry,
        };
        let Some(stops) = self.stops(&[c, c.alpha(0.4), c.alpha(0.0)]) else { return };
        unsafe {
            let Ok(brush) = self.rt.CreateRadialGradientBrush(&props, None, &stops) else { return };
            let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
            self.rt.FillRoundedRectangle(&rr, &brush);
        }
    }

    pub fn glow_round(&self, r: Rect, radius: f32, colors: &[Color], vertical: bool, size: f32) {
        let steps = size.round() as i32;
        for i in 1..=steps {
            let d = i as f32;
            let k = 1.0 - d / (size + 1.0);
            let faded: Vec<Color> = colors.iter().map(|c| c.alpha(0.22 * k * k)).collect();
            let rr = Rect::new(r.x - d + 0.5, r.y - d + 0.5, r.w + 2.0 * d - 1.0, r.h + 2.0 * d - 1.0);
            self.stroke_round_grad(rr, radius + d, 1.0, &faded, vertical);
        }
    }

    pub fn fill_circle(&self, cx: f32, cy: f32, r: f32, c: Color) {
        self.set_color(c);
        let e = D2D1_ELLIPSE { point: pt(cx, cy), radiusX: r, radiusY: r };
        unsafe { self.rt.FillEllipse(&e, &self.brush) }
    }

    pub fn line(&self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, c: Color) {
        self.set_color(c);
        unsafe { self.rt.DrawLine(pt(x1, y1), pt(x2, y2), &self.brush, width, &self.round_stroke()) }
    }

    fn round_stroke(&self) -> ID2D1StrokeStyle {
        thread_local! {
            static STROKE: RefCell<Option<ID2D1StrokeStyle>> = const { RefCell::new(None) };
        }
        STROKE.with(|s| {
            s.borrow_mut()
                .get_or_insert_with(|| {
                    let props = D2D1_STROKE_STYLE_PROPERTIES {
                        startCap: D2D1_CAP_STYLE_ROUND,
                        endCap: D2D1_CAP_STYLE_ROUND,
                        dashCap: D2D1_CAP_STYLE_ROUND,
                        lineJoin: D2D1_LINE_JOIN_ROUND,
                        miterLimit: 10.0,
                        dashStyle: D2D1_DASH_STYLE_SOLID,
                        dashOffset: 0.0,
                    };
                    with_f(|f| unsafe { f.d2d.CreateStrokeStyle(&props, None) }).expect("stroke style")
                })
                .clone()
        })
    }

    fn path(&self, points: &[(f32, f32)], closed: bool) -> Option<ID2D1PathGeometry> {
        if points.len() < 2 {
            return None;
        }
        unsafe {
            let geo = with_f(|f| f.d2d.CreatePathGeometry()).ok()?;
            let sink = geo.Open().ok()?;
            let begin = if closed { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW };
            sink.BeginFigure(pt(points[0].0, points[0].1), begin);
            let rest: Vec<D2D_POINT_2F> = points[1..].iter().map(|p| pt(p.0, p.1)).collect();
            sink.AddLines(&rest);
            sink.EndFigure(if closed { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
            sink.Close().ok()?;
            Some(geo)
        }
    }

    pub fn fill_poly(&self, points: &[(f32, f32)], c: Color) {
        if let Some(geo) = self.path(points, true) {
            self.set_color(c);
            unsafe { self.rt.FillGeometry(&geo, &self.brush, None) }
        }
    }

    pub fn polyline(&self, points: &[(f32, f32)], width: f32, c: Color) {
        if let Some(geo) = self.path(points, false) {
            self.set_color(c);
            unsafe { self.rt.DrawGeometry(&geo, &self.brush, width, &self.round_stroke()) }
        }
    }

    pub fn crescent(&self, cx: f32, cy: f32, r: f32, c: Color) {
        unsafe {
            let res: Result<ID2D1PathGeometry> = with_f(|f| {
                let a = f.d2d.CreateEllipseGeometry(&D2D1_ELLIPSE { point: pt(cx, cy), radiusX: r, radiusY: r })?;
                let b = f.d2d.CreateEllipseGeometry(&D2D1_ELLIPSE {
                    point: pt(cx + r * 0.55, cy - r * 0.35),
                    radiusX: r * 0.85,
                    radiusY: r * 0.85,
                })?;
                let geo = f.d2d.CreatePathGeometry()?;
                let sink = geo.Open()?;
                a.CombineWithGeometry(&b, D2D1_COMBINE_MODE_EXCLUDE, None, 0.25, &sink)?;
                sink.Close()?;
                Ok(geo)
            });
            if let Ok(geo) = res {
                self.set_color(c);
                self.rt.FillGeometry(&geo, &self.brush, None);
            }
        }
    }

    fn format(&self, size: f32, weight: i32, align: Align, icon: bool) -> Option<IDWriteTextFormat> {
        let key = ((size * 10.0) as u32, weight, align, icon);
        if let Some(f) = self.formats.borrow().get(&key) {
            return Some(f.clone());
        }
        let fmt = with_f(|f| unsafe {
            let family = if icon { &f.icon_font } else { &self.font };
            let fmt = f
                .dw
                .CreateTextFormat(
                    PCWSTR(family.as_ptr()),
                    None,
                    DWRITE_FONT_WEIGHT(weight),
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("tr-TR"),
                )
                .ok()?;
            let _ = fmt.SetTextAlignment(match align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            });
            let _ = fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            let _ = fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            if let Ok(sign) = f.dw.CreateEllipsisTrimmingSign(&fmt) {
                let trimming = DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                };
                let _ = fmt.SetTrimming(&trimming, &sign);
            }
            Some(fmt)
        })?;
        self.formats.borrow_mut().insert(key, fmt.clone());
        Some(fmt)
    }

    pub fn text(&self, s: &str, r: Rect, size: f32, weight: i32, c: Color, align: Align) {
        self.draw_text(s, r, size, weight, c, align, false)
    }

    pub fn glyph(&self, g: char, r: Rect, size: f32, c: Color) {
        self.draw_text(&g.to_string(), r, size, 400, c, Align::Center, true)
    }

    fn draw_text(&self, s: &str, r: Rect, size: f32, weight: i32, c: Color, align: Align, icon: bool) {
        let Some(fmt) = self.format(size, weight, align, icon) else { return };
        let text: Vec<u16> = s.encode_utf16().collect();
        self.set_color(c);
        unsafe {
            self.rt.DrawText(
                &text,
                &fmt,
                &r.d2d(),
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            )
        }
    }

    pub fn measure(&self, s: &str, size: f32, weight: i32) -> f32 {
        let Some(fmt) = self.format(size, weight, Align::Left, false) else { return 0.0 };
        let text: Vec<u16> = s.encode_utf16().collect();
        with_f(|f| unsafe {
            let Ok(layout) = f.dw.CreateTextLayout(&text, &fmt, 10000.0, 1000.0) else { return 0.0 };
            let mut m = DWRITE_TEXT_METRICS::default();
            match layout.GetMetrics(&mut m) {
                Ok(()) => m.widthIncludingTrailingWhitespace,
                Err(_) => 0.0,
            }
        })
    }

    pub fn image(&self, bmp: &ID2D1Bitmap, r: Rect, radius: f32) {
        unsafe {
            let size = bmp.GetSize();
            if size.width <= 0.0 || size.height <= 0.0 {
                return;
            }
            let Ok(brush) = self.rt.CreateBitmapBrush(bmp, None, None) else { return };
            let s = (r.w / size.width).max(r.h / size.height);
            let ox = r.x + (r.w - size.width * s) / 2.0;
            let oy = r.y + (r.h - size.height * s) / 2.0;
            brush.SetTransform(&Matrix3x2 { M11: s, M12: 0.0, M21: 0.0, M22: s, M31: ox, M32: oy });
            let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
            self.rt.FillRoundedRectangle(&rr, &brush);
        }
    }

    pub fn icon(&self, bmp: &ID2D1Bitmap, r: Rect) {
        unsafe {
            let px = bmp.GetPixelSize();
            if px.width == 0 || px.height == 0 {
                return;
            }
            let sc = self.scale;
            let (want_w, want_h) = (r.w * sc, r.h * sc);
            let (w, h) = if (px.width as f32 - want_w).abs() < 1.5 && (px.height as f32 - want_h).abs() < 1.5 {
                (px.width as f32, px.height as f32)
            } else {
                (want_w.round(), want_h.round())
            };
            let x = ((r.x + r.w / 2.0) * sc - w / 2.0).round() / sc;
            let y = ((r.y + r.h / 2.0) * sc - h / 2.0).round() / sc;
            let dst = Rect::new(x, y, w / sc, h / sc);
            self.rt.DrawBitmap(bmp, Some(&dst.d2d()), 1.0, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None);
        }
    }

    pub fn sprite(&self, bmp: &ID2D1Bitmap, src: Rect, dst: Rect, flip: bool, opacity: f32) {
        unsafe {
            if flip {
                let cx = dst.x + dst.w / 2.0;
                self.rt.SetTransform(&Matrix3x2 { M11: -1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 2.0 * cx, M32: 0.0 });
            }
            self.rt.DrawBitmap(bmp, Some(&dst.d2d()), opacity, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, Some(&src.d2d()));
            if flip {
                self.rt.SetTransform(&Matrix3x2::identity());
            }
        }
    }

    pub fn bitmap_from_icon(&self, icon: HICON) -> Option<ID2D1Bitmap> {
        with_f(|f| unsafe {
            let src = f.wic.CreateBitmapFromHICON(icon).ok()?;
            let conv = f.wic.CreateFormatConverter().ok()?;
            conv.Initialize(
                &src,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeMedianCut,
            )
            .ok()?;
            self.rt.CreateBitmapFromWicBitmap(&conv, None).ok()
        })
    }

    pub fn bitmap_from_icon_px(&self, icon: HICON, px: u32) -> Option<ID2D1Bitmap> {
        with_f(|f| unsafe {
            let src = f.wic.CreateBitmapFromHICON(icon).ok()?;
            let (mut w, mut h) = (0u32, 0u32);
            src.GetSize(&mut w, &mut h).ok()?;
            let conv = f.wic.CreateFormatConverter().ok()?;
            if w > px && px > 0 {
                let scaler = f.wic.CreateBitmapScaler().ok()?;
                scaler.Initialize(&src, px, px, WICBitmapInterpolationModeHighQualityCubic).ok()?;
                conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
            } else {
                conv.Initialize(&src, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
            }
            self.rt.CreateBitmapFromWicBitmap(&conv, None).ok()
        })
    }

    pub fn bitmap_from_shell_image(&self, h: windows::Win32::Graphics::Gdi::HBITMAP, px: u32) -> Option<ID2D1Bitmap> {
        with_f(|f| unsafe {
            let src = f.wic.CreateBitmapFromHBITMAP(h, None, WICBitmapUseAlpha).ok()?;
            let (mut w, mut hh) = (0u32, 0u32);
            src.GetSize(&mut w, &mut hh).ok()?;
            let conv = f.wic.CreateFormatConverter().ok()?;
            if w != px && px > 0 {
                let straight = f.wic.CreateFormatConverter().ok()?;
                straight.Initialize(&src, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
                let scaler = f.wic.CreateBitmapScaler().ok()?;
                scaler.Initialize(&straight, px, px, WICBitmapInterpolationModeHighQualityCubic).ok()?;
                conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
            } else {
                conv.Initialize(&src, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
            }
            self.rt.CreateBitmapFromWicBitmap(&conv, None).ok()
        })
    }

    pub fn bitmap_from_hbitmap(&self, h: windows::Win32::Graphics::Gdi::HBITMAP) -> Option<ID2D1Bitmap> {
        with_f(|f| unsafe {
            let src = f.wic.CreateBitmapFromHBITMAP(h, None, WICBitmapUsePremultipliedAlpha).ok()?;
            let conv = f.wic.CreateFormatConverter().ok()?;
            conv.Initialize(&src, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
            self.rt.CreateBitmapFromWicBitmap(&conv, None).ok()
        })
    }

    pub fn bitmap_from_bytes(&self, bytes: &[u8], max_px: u32) -> Option<ID2D1Bitmap> {
        with_f(|f| unsafe {
            let stream = f.wic.CreateStream().ok()?;
            stream.InitializeFromMemory(bytes).ok()?;
            let dec = f
                .wic
                .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
                .ok()?;
            let frame = dec.GetFrame(0).ok()?;
            let (mut w, mut h) = (0u32, 0u32);
            frame.GetSize(&mut w, &mut h).ok()?;
            if w == 0 || h == 0 {
                return None;
            }
            let k = (max_px as f32 / w.max(h) as f32).min(1.0);
            let (tw, th) = (((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1));
            let scaler = f.wic.CreateBitmapScaler().ok()?;
            scaler.Initialize(&frame, tw, th, WICBitmapInterpolationModeFant).ok()?;
            let conv = f.wic.CreateFormatConverter().ok()?;
            conv.Initialize(
                &scaler,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeMedianCut,
            )
            .ok()?;
            self.rt.CreateBitmapFromWicBitmap(&conv, None).ok()
        })
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.memdc, self.old);
            let _ = DeleteObject(self.bmp);
            let _ = DeleteDC(self.memdc);
        }
    }
}

fn pt(x: f32, y: f32) -> D2D_POINT_2F {
    D2D_POINT_2F { x, y }
}

unsafe fn create_dib(w: i32, h: i32) -> Result<(HDC, HBITMAP, HGDIOBJ)> {
    let screen = GetDC(None);
    let memdc = CreateCompatibleDC(screen);
    ReleaseDC(None, screen);
    let bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let bmp = CreateDIBSection(memdc, &bi, DIB_RGB_COLORS, &mut bits, None, 0)?;
    let old = SelectObject(memdc, bmp);
    Ok((memdc, bmp, old))
}

pub fn window_icon(hwnd: HWND) -> Option<HICON> {
    unsafe {
        for kind in [ICON_BIG, ICON_SMALL2, ICON_SMALL] {
            let mut r = 0usize;
            let ok = SendMessageTimeoutW(
                hwnd,
                WM_GETICON,
                WPARAM(kind as usize),
                LPARAM(0),
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                100,
                Some(&mut r),
            );
            if ok.0 != 0 && r != 0 {
                return Some(HICON(r as _));
            }
        }
        for idx in [GCLP_HICON, GCLP_HICONSM] {
            let r = GetClassLongPtrW(hwnd, idx);
            if r != 0 {
                return Some(HICON(r as _));
            }
        }
        None
    }
}

pub fn image_pixels(path: &str, size: u32) -> Option<Vec<u8>> {
    with_f(|f| unsafe {
        let dec = f
            .wic
            .CreateDecoderFromFilename(&windows::core::HSTRING::from(path), None, windows::Win32::Foundation::GENERIC_READ, WICDecodeMetadataCacheOnDemand)
            .ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let scaler = f.wic.CreateBitmapScaler().ok()?;
        scaler.Initialize(&frame, size, size, WICBitmapInterpolationModeFant).ok()?;
        let conv = f.wic.CreateFormatConverter().ok()?;
        conv.Initialize(&scaler, &GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
        let mut buf = vec![0u8; (size * size * 4) as usize];
        conv.CopyPixels(std::ptr::null(), size * 4, &mut buf).ok()?;
        Some(buf)
    })
}

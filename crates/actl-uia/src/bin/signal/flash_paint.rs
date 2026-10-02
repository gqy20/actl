//! Open light ribbon. Text is rasterized once; animation only composites a small light wake.
use super::paint::{ComScope, color, color_a, pixels, rounded, text_formats};
use super::task_notice::Kind;
use windows::Win32::Graphics::{
    Direct2D::Common::*, Direct2D::*, DirectWrite::*, Dxgi::Common::*, Imaging::*,
};
use windows::Win32::System::Com::*;
use windows::core::Result;

pub(super) const HEIGHT: i32 = 112;
const BEAM_Y: f32 = 88.;
pub(super) struct FlashRaster {
    pub width: i32,
    pub dpi: u32,
    pub data: Vec<u8>,
    pub accent: u32,
}

pub(super) unsafe fn raster(
    text: &str,
    dpi: u32,
    kind: Kind,
    available: i32,
) -> Result<FlashRaster> {
    let dpi = dpi.max(96);
    CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    let _com = ComScope;
    let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
    let (latin, cjk) = text_formats(&write)?;
    let width = available.clamp(300, 840);
    let title = text.encode_utf16().collect::<Vec<_>>();
    let layout = write.CreateTextLayout(&title, cjk, width as f32 - 280., 68.)?;
    let range = DWRITE_TEXT_RANGE {
        startPosition: 0,
        length: title.len() as u32,
    };
    layout.SetFontSize(30., range)?;
    layout.SetFontWeight(DWRITE_FONT_WEIGHT_SEMI_BOLD, range)?;
    layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    let ellipsis = write.CreateEllipsisTrimmingSign(cjk)?;
    layout.SetTrimming(
        &DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            ..Default::default()
        },
        &ellipsis,
    )?;
    let brand =
        write.CreateTextLayout(&"ACTL".encode_utf16().collect::<Vec<_>>(), latin, 94., 68.)?;
    brand.SetFontSize(
        24.,
        DWRITE_TEXT_RANGE {
            startPosition: 0,
            length: 4,
        },
    )?;
    let mut brand_metrics = DWRITE_TEXT_METRICS::default();
    brand.GetMetrics(&mut brand_metrics)?;
    let title_x = 142. + brand_metrics.width + 30.;
    let status = if matches!(kind, Kind::Started | Kind::Completed) {
        None
    } else {
        let label = format!(" · {}", kind.label().trim_start_matches("任务"));
        let status =
            write.CreateTextLayout(&label.encode_utf16().collect::<Vec<_>>(), cjk, 180., 68.)?;
        status.SetFontSize(
            26.,
            DWRITE_TEXT_RANGE {
                startPosition: 0,
                length: label.encode_utf16().count() as u32,
            },
        )?;
        status.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        status.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
        Some(status)
    };
    let mut status_metrics = DWRITE_TEXT_METRICS::default();
    if let Some(status) = &status {
        status.GetMetrics(&mut status_metrics)?;
    }
    // Reserve the exception suffix before trimming the task name; it must never disappear.
    let title_max = (width as f32 - title_x - 28. - status_metrics.width).max(1.);
    layout.SetMaxWidth(title_max)?;
    // The embedded Latin/CJK fonts share the same baseline, not independent vertical centers.
    layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
    brand.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
    let mut title_line = [DWRITE_LINE_METRICS::default(); 1];
    let mut brand_line = [DWRITE_LINE_METRICS::default(); 1];
    let mut count = 0;
    layout.GetLineMetrics(Some(&mut title_line), &mut count)?;
    brand.GetLineMetrics(Some(&mut brand_line), &mut count)?;
    let (pw, ph) = (pixels(width, dpi) as u32, pixels(HEIGHT, dpi) as u32);
    let wic: IWICImagingFactory =
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
    let bitmap = wic.CreateBitmap(pw, ph, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
    let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
    let target = factory.CreateWicBitmapRenderTarget(
        &bitmap,
        &D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: dpi as f32,
            dpiY: dpi as f32,
            ..Default::default()
        },
    )?;
    let point = |x, y| {
        let mut p = D2D1_BEZIER_SEGMENT::default().point1;
        p.X = x;
        p.Y = y;
        p
    };
    let stops = [
        D2D1_GRADIENT_STOP {
            position: 0.,
            color: color_a(0x27292B, 0.),
        },
        D2D1_GRADIENT_STOP {
            position: 0.12,
            color: color_a(0x27292B, 0.92),
        },
        D2D1_GRADIENT_STOP {
            position: 0.8,
            color: color_a(0x27292B, 0.94),
        },
        D2D1_GRADIENT_STOP {
            position: 1.,
            color: color_a(0x27292B, 0.),
        },
    ];
    let collection =
        target.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
    let surface = target.CreateLinearGradientBrush(
        &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: point(16., 0.),
            endPoint: point(width as f32 - 12., 0.),
        },
        None,
        &collection,
    )?;
    let beam_gradient = |rgb| -> Result<ID2D1LinearGradientBrush> {
        let stops = [(0., 0.75), (0.24, 1.), (0.84, 0.6), (1., 0.)].map(|(position, alpha)| {
            D2D1_GRADIENT_STOP {
                position,
                color: color_a(rgb, alpha),
            }
        });
        let collection =
            target.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
        target.CreateLinearGradientBrush(
            &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                startPoint: point(0., 0.),
                endPoint: point(width as f32, 0.),
            },
            None,
            &collection,
        )
    };
    let beam = beam_gradient(kind.color())?;
    let core = beam_gradient(0xEEF7FF)?;
    let brush = target.CreateSolidColorBrush(&color(kind.color()), None)?;
    target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
    target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
    let ribbon = factory.CreatePathGeometry()?;
    let sink = ribbon.Open()?;
    sink.BeginFigure(point(132., 86.), D2D1_FIGURE_BEGIN_HOLLOW);
    sink.AddBeziers(&[
        D2D1_BEZIER_SEGMENT {
            point1: point(100., 86.),
            point2: point(84., 26.),
            point3: point(53., 31.),
        },
        D2D1_BEZIER_SEGMENT {
            point1: point(6., 32.),
            point2: point(6., 82.),
            point3: point(45., 79.),
        },
        D2D1_BEZIER_SEGMENT {
            point1: point(80., 79.),
            point2: point(78., 56.),
            point3: point(111., 78.),
        },
        D2D1_BEZIER_SEGMENT {
            point1: point(128., 89.),
            point2: point(149., 88.),
            point3: point(width as f32 - 20., BEAM_Y),
        },
    ]);
    sink.EndFigure(D2D1_FIGURE_END_OPEN);
    sink.Close()?;
    target.BeginDraw();
    target.Clear(Some(&D2D1_COLOR_F::default()));
    target.FillRoundedRectangle(&rounded(16., 17., width as f32 - 28., 72., 8.), &surface);
    // Multi-width strokes make a tight luminous halo, without an enclosing frame.
    for (stroke, opacity) in [(18., 0.018), (11., 0.045), (6., 0.12), (2.2, 0.95)] {
        beam.SetOpacity(opacity);
        target.DrawGeometry(&ribbon, &beam, stroke, None);
    }
    core.SetOpacity(0.9);
    target.DrawGeometry(&ribbon, &core, 0.8, None);
    // Terminal states keep an unambiguous symbol while the surrounding ribbon remains open.
    let strokes: &[(f32, f32, f32, f32)] = match kind {
        Kind::Started => &[],
        Kind::Completed => &[(45., 53., 53., 61.), (53., 61., 68., 44.)],
        Kind::Failed => &[(47., 44., 65., 62.), (47., 62., 65., 44.)],
        Kind::Stopped => &[
            (47., 44., 65., 44.),
            (65., 44., 65., 62.),
            (65., 62., 47., 62.),
            (47., 62., 47., 44.),
        ],
        Kind::Paused => &[(50., 44., 50., 62.), (62., 44., 62., 62.)],
        Kind::Attention => &[(56., 42., 56., 55.), (56., 60., 56., 63.)],
    };
    brush.SetColor(&color(kind.color()));
    for &(x1, y1, x2, y2) in strokes {
        target.DrawLine(point(x1, y1), point(x2, y2), &brush, 2.5, None);
    }
    brush.SetColor(&color(0xB4BAC0));
    target.DrawTextLayout(
        point(142., 63. - brand_line[0].baseline),
        &brand,
        &brush,
        D2D1_DRAW_TEXT_OPTIONS_CLIP,
    );
    brush.SetColor(&color(0xE8EBED));
    target.DrawTextLayout(
        point(title_x, 63. - title_line[0].baseline),
        &layout,
        &brush,
        D2D1_DRAW_TEXT_OPTIONS_CLIP,
    );
    if let Some(status) = status {
        let mut line = [DWRITE_LINE_METRICS::default(); 1];
        status.GetLineMetrics(Some(&mut line), &mut count)?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut metrics)?;
        brush.SetColor(&color(kind.color()));
        target.DrawTextLayout(
            point(
                title_x + metrics.width.min(title_max),
                63. - line[0].baseline,
            ),
            &status,
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
        );
    }
    target.EndDraw(None, None)?;
    let mut data = vec![0; pw as usize * ph as usize * 4];
    bitmap.CopyPixels(std::ptr::null(), pw * 4, &mut data)?;
    Ok(FlashRaster {
        width,
        dpi,
        data,
        accent: kind.color(),
    })
}

/// Source-over in premultiplied alpha, bounded to the thin beam below the glyphs.
pub(super) fn illuminate(raster: &FlashRaster, phase: u8) -> Vec<u8> {
    let mut data = raster.data.clone();
    let scale = raster.dpi as f32 / 96.;
    let pw = pixels(raster.width, raster.dpi) as usize;
    let ph = pixels(HEIGHT, raster.dpi) as usize;
    let t = phase.min(63) as f32 / 63.;
    let x = (140. + (raster.width as f32 - 170.) * t) * scale;
    let y = BEAM_Y * scale;
    let halo = [
        ((raster.accent & 255) as f32),
        (((raster.accent >> 8) & 255) as f32),
        (((raster.accent >> 16) & 255) as f32),
    ];
    let left = (x - 100. * scale).max(0.) as usize;
    let right = ((x + 22. * scale).ceil() as usize).min(pw);
    let top = (y - 14. * scale).max(0.) as usize;
    let bottom = ((y + 14. * scale).ceil() as usize).min(ph);
    for py in top..bottom {
        for px in left..right {
            let dx = (px as f32 + 0.5 - x) / scale;
            let dy = (py as f32 + 0.5 - y) / scale;
            let glow = 0.3 * (-dx * dx / 450. - dy * dy / 28.).exp();
            let core = 0.95 * (-dx * dx / 6. - dy * dy / 2.).exp();
            let wake = if dx < 0. {
                0.26 * (dx / 30. - dy * dy / 2.5).exp()
            } else {
                0.
            };
            let alpha = (glow + core + wake).min(1.);
            let offset = (py * pw + px) * 4;
            for (c, channel) in halo.iter().enumerate() {
                let source = channel * (1. - core) + 250. * core;
                data[offset + c] =
                    (source * alpha + data[offset + c] as f32 * (1. - alpha)).round() as u8;
            }
            data[offset + 3] =
                (255. * alpha + data[offset + 3] as f32 * (1. - alpha)).round() as u8;
        }
    }
    data
}

using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Drawing.Drawing2D;
using System.IO;
using System.Runtime.InteropServices;

public static class MetafileReference
{
    [StructLayout(LayoutKind.Sequential)] internal struct POINT { public int x, y; public POINT(int x, int y) { this.x = x; this.y = y; } }
    [StructLayout(LayoutKind.Sequential)] internal struct RECT { public int left, top, right, bottom; }
    [StructLayout(LayoutKind.Sequential)] internal struct METAFILEPICT { public int mm, xExt, yExt; public IntPtr hMF; }
    [StructLayout(LayoutKind.Sequential)] internal struct BITMAPINFOHEADER {
        public uint size; public int width, height; public ushort planes, bitCount;
        public uint compression, sizeImage; public int xPelsPerMeter, yPelsPerMeter;
        public uint colorsUsed, colorsImportant;
    }
    [StructLayout(LayoutKind.Sequential)] internal struct XFORM {
        public float eM11, eM12, eM21, eM22, eDx, eDy;
    }

    [DllImport("gdi32.dll", CharSet = CharSet.Unicode)] static extern IntPtr CreateMetaFile(string fileName);
    [DllImport("gdi32.dll")] static extern IntPtr CloseMetaFile(IntPtr hdc);
    [DllImport("gdi32.dll")] static extern bool DeleteMetaFile(IntPtr hmf);
    [DllImport("gdi32.dll", CharSet = CharSet.Unicode)] static extern IntPtr GetMetaFile(string fileName);
    [DllImport("gdi32.dll")] static extern bool PlayMetaFile(IntPtr hdc, IntPtr hmf);
    [DllImport("gdi32.dll")] static extern int SetMapMode(IntPtr hdc, int mode);
    [DllImport("gdi32.dll")] static extern bool SetWindowOrgEx(IntPtr hdc, int x, int y, IntPtr old);
    [DllImport("gdi32.dll")] static extern bool SetWindowExtEx(IntPtr hdc, int x, int y, IntPtr old);
    [DllImport("gdi32.dll")] static extern bool SetViewportOrgEx(IntPtr hdc, int x, int y, IntPtr old);
    [DllImport("gdi32.dll")] static extern bool SetViewportExtEx(IntPtr hdc, int x, int y, IntPtr old);
    [DllImport("gdi32.dll")] static extern bool MoveToEx(IntPtr hdc, int x, int y, IntPtr old);
    [DllImport("gdi32.dll")] static extern bool LineTo(IntPtr hdc, int x, int y);
    [DllImport("gdi32.dll")] static extern bool Rectangle(IntPtr hdc, int left, int top, int right, int bottom);
    [DllImport("gdi32.dll")] static extern bool Ellipse(IntPtr hdc, int left, int top, int right, int bottom);
    [DllImport("gdi32.dll")] static extern bool Arc(IntPtr hdc, int left, int top, int right, int bottom, int sx, int sy, int ex, int ey);
    [DllImport("gdi32.dll")] static extern bool Pie(IntPtr hdc, int left, int top, int right, int bottom, int sx, int sy, int ex, int ey);
    [DllImport("gdi32.dll")] static extern bool Chord(IntPtr hdc, int left, int top, int right, int bottom, int sx, int sy, int ex, int ey);
    [DllImport("gdi32.dll")] static extern int SetArcDirection(IntPtr hdc, int direction);
    [DllImport("gdi32.dll")] static extern int SaveDC(IntPtr hdc);
    [DllImport("gdi32.dll")] static extern bool RestoreDC(IntPtr hdc, int savedDc);
    [DllImport("gdi32.dll")] static extern bool PolyPolygon(IntPtr hdc, POINT[] points, int[] counts, int polygons);
    [DllImport("gdi32.dll")] static extern int SetPolyFillMode(IntPtr hdc, int mode);
    [DllImport("gdi32.dll")] static extern bool TextOut(IntPtr hdc, int x, int y, string text, int length);
    [DllImport("gdi32.dll")] static extern uint SetTextAlign(IntPtr hdc, uint align);
    [DllImport("gdi32.dll")] static extern int SetStretchBltMode(IntPtr hdc, int mode);
    [DllImport("gdi32.dll")] static extern int SetGraphicsMode(IntPtr hdc, int mode);
    [DllImport("gdi32.dll")] static extern bool SetWorldTransform(IntPtr hdc, ref XFORM transform);
    [DllImport("gdi32.dll")] static extern bool ModifyWorldTransform(IntPtr hdc, ref XFORM transform, uint mode);
    [DllImport("gdi32.dll")] static extern int StretchDIBits(IntPtr hdc, int xDest, int yDest, int destWidth, int destHeight, int xSrc, int ySrc, int srcWidth, int srcHeight, byte[] bits, ref BITMAPINFOHEADER info, uint usage, uint rop);
    [DllImport("gdi32.dll")] static extern IntPtr CreatePen(int style, int width, uint color);
    [DllImport("gdi32.dll")] static extern IntPtr CreateSolidBrush(uint color);
    [DllImport("gdi32.dll")] static extern IntPtr SelectObject(IntPtr hdc, IntPtr obj);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr obj);
    [DllImport("gdi32.dll")] static extern IntPtr SetWinMetaFileBits(uint size, byte[] bytes, IntPtr referenceHdc, ref METAFILEPICT picture);
    [DllImport("gdi32.dll")] static extern bool PlayEnhMetaFile(IntPtr hdc, IntPtr hemf, ref RECT rect);
    [DllImport("gdi32.dll")] static extern bool DeleteEnhMetaFile(IntPtr hemf);

    static uint Rgb(byte r, byte g, byte b) { return (uint)(r | (g << 8) | (b << 16)); }

    public static void Generate(string directory)
    {
        Directory.CreateDirectory(directory);
        GenerateVector(Path.Combine(directory, "windows-gdi-vector.wmf"));
        GenerateArcs(Path.Combine(directory, "windows-gdi-arcs.wmf"));
        GenerateWmfArcMatrix(Path.Combine(directory, "windows-gdi-arc-matrix.wmf"));
        GeneratePolygons(Path.Combine(directory, "windows-gdi-polypolygon.wmf"));
        GenerateText(Path.Combine(directory, "windows-gdi-text.wmf"));
        GenerateBitmap(Path.Combine(directory, "windows-gdi-bitmap.wmf"));
        GenerateLargeVector(Path.Combine(directory, "windows-gdi-large-vector.wmf"));
        GenerateEnhanced(Path.Combine(directory, "windows-gdiplus-emf.emf"), EmfType.EmfOnly);
        GenerateEnhancedMapping(Path.Combine(directory, "windows-emf-mapping.emf"));
        GenerateEnhancedText(Path.Combine(directory, "windows-emf-text.emf"));
        GenerateEnhancedPaths(Path.Combine(directory, "windows-emf-paths.emf"));
        GenerateEnhancedBitmap(Path.Combine(directory, "windows-emf-bitmap.emf"));
        GenerateEnhancedState(Path.Combine(directory, "windows-emf-state.emf"));
        GenerateEnhancedAffine(Path.Combine(directory, "windows-emf-affine.emf"));
        GenerateEnhancedArcMatrix(Path.Combine(directory, "windows-emf-arc-matrix.emf"));
        GenerateEnhanced(Path.Combine(directory, "windows-gdiplus-emfplus-only.emf"), EmfType.EmfPlusOnly);
        GenerateEnhanced(Path.Combine(directory, "windows-gdiplus-emfplus.emf"), EmfType.EmfPlusDual);
    }

    public static void GenerateAffine(string directory)
    {
        Directory.CreateDirectory(directory);
        GenerateEnhancedAffine(Path.Combine(directory, "windows-emf-affine.emf"));
    }

    public static void GenerateArcMatrix(string directory)
    {
        Directory.CreateDirectory(directory);
        GenerateWmfArcMatrix(Path.Combine(directory, "windows-gdi-arc-matrix.wmf"));
        GenerateEnhancedArcMatrix(Path.Combine(directory, "windows-emf-arc-matrix.emf"));
    }

    public static void GenerateEmfPlusOnly(string directory)
    {
        Directory.CreateDirectory(directory);
        GenerateEnhanced(Path.Combine(directory, "windows-gdiplus-emfplus-only.emf"), EmfType.EmfPlusOnly);
    }

    public static void GenerateEmfPlusP1(string directory)
    {
        Directory.CreateDirectory(directory);
        GenerateEmfPlusVectors(Path.Combine(directory, "windows-gdiplus-emfplus-p1-vectors.emf"));
        GenerateEmfPlusImages(Path.Combine(directory, "windows-gdiplus-emfplus-p1-images.emf"));
        GenerateEmfPlusText(Path.Combine(directory, "windows-gdiplus-emfplus-p1-text.emf"));
        GenerateEmfPlusRegions(Path.Combine(directory, "windows-gdiplus-emfplus-p1-regions.emf"));
        GenerateEmfPlusState(Path.Combine(directory, "windows-gdiplus-emfplus-p1-state.emf"));
    }

    static void WithEmfPlus(string path, Action<Graphics> draw)
    {
        using (var reference = new Bitmap(1, 1))
        using (var referenceGraphics = Graphics.FromImage(reference))
        using (var stream = File.Create(path))
        {
            IntPtr hdc = referenceGraphics.GetHdc();
            try {
                using (var metafile = new Metafile(stream, hdc, new RectangleF(0, 0, 600, 400), MetafileFrameUnit.Pixel, EmfType.EmfPlusOnly))
                using (var graphics = Graphics.FromImage(metafile)) draw(graphics);
            }
            finally { referenceGraphics.ReleaseHdc(hdc); }
        }
    }

    static Bitmap QualificationBitmap()
    {
        var bitmap = new Bitmap(8, 6, PixelFormat.Format32bppArgb);
        using (var graphics = Graphics.FromImage(bitmap))
        {
            graphics.Clear(Color.Transparent);
            using var red = new SolidBrush(Color.FromArgb(220, 230, 30, 50));
            using var blue = new SolidBrush(Color.FromArgb(180, 20, 90, 230));
            graphics.FillRectangle(red, 0, 0, 4, 6);
            graphics.FillEllipse(blue, 2, 0, 6, 6);
        }
        return bitmap;
    }

    static void GenerateEmfPlusVectors(string path)
    {
        using var textureImage = QualificationBitmap();
        WithEmfPlus(path, graphics =>
        {
            graphics.SmoothingMode = SmoothingMode.AntiAlias;
            using var gradient = new LinearGradientBrush(new Rectangle(20, 20, 210, 100), Color.Red, Color.Blue, 25f);
            gradient.Blend = new Blend { Positions = new[] { 0f, .35f, 1f }, Factors = new[] { 0f, .8f, 1f } };
            gradient.TranslateTransform(8, 5);
            graphics.FillRectangle(gradient, 20, 20, 210, 100);
            using var texture = new TextureBrush(textureImage, WrapMode.TileFlipXY);
            texture.TranslateTransform(12, 7);
            graphics.FillEllipse(texture, 270, 20, 190, 110);
            using var pen = new Pen(Color.DarkGreen, 5) { DashPattern = new[] { 3f, 1f, 1f, 1f }, DashOffset = 0.5f, LineJoin = LineJoin.Round };
            graphics.DrawCurve(pen, new[] { new PointF(30, 200), new PointF(150, 145), new PointF(270, 250), new PointF(420, 170), new PointF(560, 260) }, .6f);
            graphics.DrawClosedCurve(pen, new[] { new PointF(60, 300), new PointF(170, 275), new PointF(220, 360), new PointF(90, 370) }, .4f, FillMode.Winding);
        });
    }

    static void GenerateEmfPlusImages(string path)
    {
        using var source = QualificationBitmap();
        using var pngStream = new MemoryStream();
        source.Save(pngStream, ImageFormat.Png);
        pngStream.Position = 0;
        using var pngImage = new Bitmap(pngStream);
        using var jpegStream = new MemoryStream();
        source.Save(jpegStream, ImageFormat.Jpeg);
        jpegStream.Position = 0;
        using var jpegImage = new Bitmap(jpegStream);
        WithEmfPlus(path, graphics =>
        {
            graphics.InterpolationMode = InterpolationMode.NearestNeighbor;
            graphics.DrawImage(pngImage, new Rectangle(30, 30, 220, 150), 1, 1, 6, 4, GraphicsUnit.Pixel);
            graphics.DrawImage(jpegImage, new[] { new PointF(320, 35), new PointF(560, 70), new PointF(290, 220) }, new RectangleF(0, 0, 8, 6), GraphicsUnit.Pixel);
            using var attributes = new ImageAttributes();
            attributes.SetWrapMode(WrapMode.TileFlipXY, Color.Transparent, false);
            var matrix = new ColorMatrix { Matrix33 = .55f };
            attributes.SetColorMatrix(matrix);
            graphics.DrawImage(source, new Rectangle(80, 250, 360, 110), 0, 0, 8, 6, GraphicsUnit.Pixel, attributes);
        });
    }

    static void GenerateEmfPlusText(string path)
    {
        WithEmfPlus(path, graphics =>
        {
            using var font = new Font("Arial", 25, FontStyle.Bold | FontStyle.Italic, GraphicsUnit.Point);
            using var format = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center, Trimming = StringTrimming.EllipsisWord };
            graphics.TranslateTransform(280, 170);
            graphics.RotateTransform(-24);
            graphics.ScaleTransform(1.15f, .85f);
            graphics.DrawString("Wrapped Unicode Ω 日本 text for EMF+", font, Brushes.Navy, new RectangleF(-190, -70, 380, 140), format);
        });
    }

    static void GenerateEmfPlusRegions(string path)
    {
        WithEmfPlus(path, graphics =>
        {
            using var region = new Region(new Rectangle(30, 30, 300, 250));
            region.Union(new Rectangle(280, 80, 260, 230));
            region.Exclude(new Rectangle(190, 120, 200, 100));
            graphics.SetClip(region, CombineMode.Replace);
            using var brush = new LinearGradientBrush(new Rectangle(0, 0, 600, 400), Color.Gold, Color.Purple, LinearGradientMode.ForwardDiagonal);
            graphics.FillRectangle(brush, 0, 0, 600, 400);
        });
    }

    static void GenerateEmfPlusState(string path)
    {
        WithEmfPlus(path, graphics =>
        {
            graphics.SmoothingMode = SmoothingMode.AntiAlias;
            using var blue = new SolidBrush(Color.FromArgb(210, 40, 100, 220));
            using var orange = new SolidBrush(Color.FromArgb(220, 240, 120, 20));
            using var pen = new Pen(Color.DarkSlateBlue, 4);

            var saved = graphics.Save();
            graphics.TranslateTransform(115, 80);
            graphics.RotateTransform(28);
            graphics.ScaleTransform(1.25f, .7f);
            graphics.FillRectangle(blue, -70, -35, 140, 70);
            graphics.Restore(saved);
            graphics.DrawRectangle(pen, 20, 145, 190, 90);

            var container = graphics.BeginContainer(
                new RectangleF(300, 35, 240, 150),
                new RectangleF(0, 0, 144, 72),
                GraphicsUnit.Point);
            graphics.FillEllipse(orange, 18, 8, 108, 56);
            graphics.EndContainer(container);

            using var clipPath = new GraphicsPath();
            clipPath.AddEllipse(280, 225, 260, 145);
            graphics.SetClip(clipPath, CombineMode.Replace);
            using var path = new GraphicsPath();
            path.AddBezier(250, 365, 335, 175, 455, 410, 575, 225);
            path.AddPie(315, 230, 180, 120, 20, 245);
            graphics.FillPath(blue, path);
            graphics.DrawPath(pen, path);
        });
    }

    static void WithWmf(string path, Action<IntPtr> draw)
    {
        string standard = path + ".standard";
        IntPtr dc = CreateMetaFile(standard);
        if (dc == IntPtr.Zero) throw new InvalidOperationException("CreateMetaFile failed");
        draw(dc);
        IntPtr metafile = CloseMetaFile(dc);
        if (metafile == IntPtr.Zero) throw new InvalidOperationException("CloseMetaFile failed");
        DeleteMetaFile(metafile);
        byte[] body = File.ReadAllBytes(standard);
        File.Delete(standard);
        byte[] header;
        using (var stream = new MemoryStream())
        using (var output = new BinaryWriter(stream))
        {
            output.Write(0x9AC6CDD7u); output.Write((ushort)0);
            output.Write((short)0); output.Write((short)0); output.Write((short)600); output.Write((short)400);
            output.Write((ushort)1440); output.Write(0u);
            output.Flush(); header = stream.ToArray();
            ushort checksum = 0;
            for (int i = 0; i < 20; i += 2) checksum ^= BitConverter.ToUInt16(header, i);
            using (var file = new BinaryWriter(File.Create(path))) { file.Write(header); file.Write(checksum); file.Write(body); }
        }
    }

    static void GenerateVector(string path)
    {
        WithWmf(path, dc => {
            IntPtr pen = CreatePen(0, 0, Rgb(20, 70, 180));
            IntPtr brush = CreateSolidBrush(Rgb(230, 190, 30));
            IntPtr oldPen = SelectObject(dc, pen), oldBrush = SelectObject(dc, brush);
            Rectangle(dc, 30, 30, 260, 170); Ellipse(dc, 300, 30, 550, 170);
            MoveToEx(dc, 30, 220, IntPtr.Zero); LineTo(dc, 550, 350);
            SelectObject(dc, oldPen); SelectObject(dc, oldBrush); DeleteObject(pen); DeleteObject(brush);
        });
    }

    static void GenerateArcs(string path)
    {
        WithWmf(path, dc => {
            IntPtr pen = CreatePen(0, 3, Rgb(160, 20, 40)); IntPtr old = SelectObject(dc, pen);
            Arc(dc, 20, 20, 180, 140, 180, 80, 100, 20);
            Pie(dc, 210, 20, 370, 140, 370, 80, 290, 140);
            Chord(dc, 400, 20, 580, 140, 490, 20, 400, 80);
            Arc(dc, 40, 190, 560, 370, 560, 280, 559, 279);
            SelectObject(dc, old); DeleteObject(pen);
        });
    }

    static void DrawArcMatrix(IntPtr dc, bool includeInvertedMappings)
    {
        IntPtr pen = CreatePen(0, 2, Rgb(35, 75, 170));
        IntPtr brush = CreateSolidBrush(Rgb(235, 190, 55));
        IntPtr oldPen = SelectObject(dc, pen), oldBrush = SelectObject(dc, brush);
        // 90, 180, >180, and near-full sweeps distributed across all quadrants.
        Arc(dc, 15, 15, 135, 95, 135, 55, 75, 15);
        Arc(dc, 155, 15, 275, 95, 275, 55, 155, 55);
        Arc(dc, 295, 15, 415, 95, 295, 55, 355, 15);
        Arc(dc, 435, 15, 585, 95, 585, 55, 584, 54);
        Arc(dc, 15, 115, 165, 205, 90, 115, 165, 160);
        Arc(dc, 175, 115, 325, 205, 250, 205, 175, 160);
        Pie(dc, 335, 115, 455, 205, 455, 160, 395, 115);
        Chord(dc, 465, 115, 585, 205, 525, 115, 465, 160);

        // Explicit clockwise state plus a representative coincident-endpoint case.
        SetArcDirection(dc, 2);
        Arc(dc, 15, 225, 165, 305, 165, 265, 90, 225);
        Pie(dc, 175, 225, 325, 305, 250, 225, 250, 225);
        SetArcDirection(dc, 1);

        if (includeInvertedMappings)
        {
            // EMF preserves explicit viewport inversion reliably in the reference API.
            int saved = SaveDC(dc);
            SetMapMode(dc, 8); SetWindowOrgEx(dc, 0, 0, IntPtr.Zero); SetWindowExtEx(dc, 600, 400, IntPtr.Zero);
            SetViewportOrgEx(dc, 600, 0, IntPtr.Zero); SetViewportExtEx(dc, -600, 400, IntPtr.Zero);
            Arc(dc, 265, 225, 385, 305, 385, 265, 325, 225);
            RestoreDC(dc, saved);
            saved = SaveDC(dc);
            SetMapMode(dc, 8); SetWindowOrgEx(dc, 0, 0, IntPtr.Zero); SetWindowExtEx(dc, 600, 400, IntPtr.Zero);
            SetViewportOrgEx(dc, 0, 400, IntPtr.Zero); SetViewportExtEx(dc, 600, -400, IntPtr.Zero);
            Pie(dc, 405, 95, 525, 175, 525, 135, 465, 95);
            RestoreDC(dc, saved);
            saved = SaveDC(dc);
            SetMapMode(dc, 8); SetWindowOrgEx(dc, 0, 0, IntPtr.Zero); SetWindowExtEx(dc, 600, 400, IntPtr.Zero);
            SetViewportOrgEx(dc, 600, 400, IntPtr.Zero); SetViewportExtEx(dc, -600, -400, IntPtr.Zero);
            Chord(dc, 15, 15, 135, 95, 135, 55, 75, 15);
            RestoreDC(dc, saved);
        }

        SelectObject(dc, oldPen); SelectObject(dc, oldBrush); DeleteObject(pen); DeleteObject(brush);
    }

    static void GenerateWmfArcMatrix(string path) { WithWmf(path, dc => DrawArcMatrix(dc, false)); }

    static void GenerateEnhancedArcMatrix(string path)
    {
        WithEnhanced(path, graphics => {
            IntPtr hdc = graphics.GetHdc();
            try { DrawArcMatrix(hdc, true); }
            finally { graphics.ReleaseHdc(hdc); }
        });
    }

    static void GeneratePolygons(string path)
    {
        WithWmf(path, dc => {
            IntPtr brush = CreateSolidBrush(Rgb(50, 170, 90)); IntPtr old = SelectObject(dc, brush);
            POINT[] p = { new POINT(30,30), new POINT(270,30), new POINT(270,270), new POINT(30,270),
                          new POINT(90,90), new POINT(90,210), new POINT(210,210), new POINT(210,90),
                          new POINT(320,40), new POINT(560,150), new POINT(320,260), new POINT(430,150) };
            SetPolyFillMode(dc, 1); PolyPolygon(dc, p, new[] { 4, 4, 4 }, 3);
            SelectObject(dc, old); DeleteObject(brush);
        });
    }

    static void GenerateText(string path)
    {
        WithWmf(path, dc => {
            SetTextAlign(dc, 0); TextOut(dc, 30, 40, "Left & top", 10);
            SetTextAlign(dc, 6); TextOut(dc, 300, 140, "Centered", 8);
            SetTextAlign(dc, 2 | 24); TextOut(dc, 560, 250, "Right baseline", 14);
        });
    }

    static void GenerateBitmap(string path)
    {
        WithWmf(path, dc => {
            BITMAPINFOHEADER info = new BITMAPINFOHEADER {
                size = 40, width = 2, height = 2, planes = 1, bitCount = 24,
                compression = 0, sizeImage = 16
            };
            // Bottom-up rows, BGR pixels, each row padded to four bytes.
            byte[] pixels = { 255,0,0, 255,255,255, 0,0, 0,0,255, 0,255,0, 0,0 };
            SetStretchBltMode(dc, 3);
            StretchDIBits(dc, 40, 40, 220, 140, 0, 0, 2, 2, pixels, ref info, 0, 0x00CC0020);
            StretchDIBits(dc, 560, 220, -220, 140, 0, 0, 2, 2, pixels, ref info, 0, 0x00CC0020);
        });
    }

    static void GenerateEnhanced(string path, EmfType type)
    {
        using (var reference = new Bitmap(1, 1))
        using (var referenceGraphics = Graphics.FromImage(reference))
        using (var stream = File.Create(path))
        {
            IntPtr hdc = referenceGraphics.GetHdc();
            try {
                using (var metafile = new Metafile(stream, hdc, new RectangleF(0, 0, 600, 400), MetafileFrameUnit.Pixel, type))
                using (var graphics = Graphics.FromImage(metafile))
                using (var pen = new System.Drawing.Pen(Color.Navy, 3))
                using (var brush = new SolidBrush(Color.Goldenrod))
                {
                    graphics.DrawEllipse(pen, 40, 40, 300, 180);
                    graphics.FillRectangle(brush, 360, 70, 180, 130);
                    graphics.DrawString(type.ToString(), SystemFonts.DefaultFont, Brushes.Black, 50, 280);
                }
            } finally { referenceGraphics.ReleaseHdc(hdc); }
        }
    }

    static void WithEnhanced(string path, Action<Graphics> draw)
    {
        using (var reference = new Bitmap(1, 1))
        using (var referenceGraphics = Graphics.FromImage(reference))
        using (var stream = File.Create(path))
        {
            IntPtr hdc = referenceGraphics.GetHdc();
            try {
                using (var metafile = new Metafile(stream, hdc, new RectangleF(0, 0, 600, 400), MetafileFrameUnit.Pixel, EmfType.EmfOnly))
                using (var graphics = Graphics.FromImage(metafile)) draw(graphics);
            }
            finally { referenceGraphics.ReleaseHdc(hdc); }
        }
    }

    static void GenerateEnhancedMapping(string path)
    {
        WithEnhanced(path, graphics => {
            graphics.TranslateTransform(80, 40);
            graphics.ScaleTransform(1.5f, 0.75f);
            graphics.RotateTransform(12);
            using (var pen = new System.Drawing.Pen(Color.DarkGreen, 4)) {
                graphics.DrawRectangle(pen, 20, 30, 220, 130);
                graphics.DrawLine(pen, 0, 0, 280, 200);
            }
        });
    }

    static void GenerateEnhancedText(string path)
    {
        WithEnhanced(path, graphics => {
            graphics.TextRenderingHint = System.Drawing.Text.TextRenderingHint.SingleBitPerPixelGridFit;
            using (var font = new System.Drawing.Font("Arial", 24, FontStyle.Bold | FontStyle.Italic)) {
                graphics.DrawString("Unicode Ω Ж 日本", font, Brushes.Navy, 35, 55);
                graphics.DrawString("Aligned text", font, Brushes.DarkRed, new RectangleF(250, 180, 300, 100), new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center });
            }
        });
    }

    static void GenerateEnhancedPaths(string path)
    {
        WithEnhanced(path, graphics => {
            using (var pathShape = new GraphicsPath())
            using (var pen = new System.Drawing.Pen(Color.Purple, 5))
            using (var brush = new SolidBrush(Color.FromArgb(255, 230, 180, 40))) {
                pathShape.StartFigure();
                pathShape.AddBezier(40, 260, 130, 20, 330, 20, 410, 260);
                pathShape.AddLine(410, 260, 40, 260);
                pathShape.CloseFigure();
                graphics.FillPath(brush, pathShape);
                graphics.DrawPath(pen, pathShape);
            }
        });
    }

    static void GenerateEnhancedBitmap(string path)
    {
        using (var source = new Bitmap(3, 2, PixelFormat.Format24bppRgb)) {
            source.SetPixel(0, 0, Color.Red); source.SetPixel(1, 0, Color.Green); source.SetPixel(2, 0, Color.Blue);
            source.SetPixel(0, 1, Color.Cyan); source.SetPixel(1, 1, Color.Magenta); source.SetPixel(2, 1, Color.Yellow);
            WithEnhanced(path, graphics => {
                graphics.InterpolationMode = InterpolationMode.NearestNeighbor;
                graphics.DrawImage(source, new System.Drawing.Rectangle(40, 40, 240, 160));
                graphics.DrawImage(source, new System.Drawing.Rectangle(540, 230, -240, 130));
            });
        }
    }

    static void GenerateEnhancedState(string path)
    {
        WithEnhanced(path, graphics => {
            using (var blue = new System.Drawing.Pen(Color.Blue, 3))
            using (var red = new System.Drawing.Pen(Color.Red, 8)) {
                graphics.DrawLine(blue, 20, 40, 560, 40);
                GraphicsState saved = graphics.Save();
                graphics.TranslateTransform(70, 100);
                graphics.SetClip(new System.Drawing.Rectangle(0, 0, 300, 140));
                graphics.DrawEllipse(red, 0, 0, 420, 180);
                graphics.Restore(saved);
                graphics.DrawRectangle(blue, 40, 260, 500, 90);
            }
        });
    }

    static void GenerateEnhancedAffine(string path)
    {
        WithEnhanced(path, graphics =>
        {
                graphics.SmoothingMode = SmoothingMode.None;
                graphics.InterpolationMode = InterpolationMode.NearestNeighbor;
                using (var pen = new System.Drawing.Pen(Color.DarkBlue, 4))
                using (var brush = new SolidBrush(Color.Goldenrod))
                using (var shear = new Matrix(1.0f, 0.20f, 0.35f, 1.0f, 320.0f, 35.0f))
                {
                    graphics.TranslateTransform(145, 45);
                    graphics.RotateTransform(30);
                    graphics.FillRectangle(brush, 0, 0, 120, 70);
                    graphics.DrawRectangle(pen, 0, 0, 120, 70);
                    graphics.DrawEllipse(pen, 10, 85, 130, 70);

                    graphics.Transform = shear;
                    GraphicsState clipped = graphics.Save();
                    graphics.SetClip(new System.Drawing.Rectangle(0, 0, 180, 125));
                    graphics.DrawEllipse(pen, -20, -20, 230, 170);
                    graphics.Restore(clipped);

                    graphics.ResetTransform();
                    graphics.TranslateTransform(45, 315, MatrixOrder.Append);
                    graphics.ScaleTransform(1.4f, 0.7f, MatrixOrder.Prepend);
                    graphics.DrawLine(pen, 0, 0, 100, 0);
                    graphics.ResetTransform();
                    graphics.TranslateTransform(300, 315, MatrixOrder.Append);
                    graphics.ScaleTransform(1.4f, 0.7f, MatrixOrder.Append);
                    graphics.DrawLine(pen, 0, 0, 100, 0);

                    BITMAPINFOHEADER info = new BITMAPINFOHEADER {
                        size = 40, width = 3, height = 2, planes = 1, bitCount = 24,
                        compression = 0, sizeImage = 24
                    };
                    byte[] pixels = {
                        255,255,0, 255,0,255, 0,255,255, 0,0,0,
                        0,0,255, 0,255,0, 255,0,0, 0,0,0
                    };
                    IntPtr hdc = graphics.GetHdc();
                    try {
                        SetGraphicsMode(hdc, 2);
                        IntPtr transformPen = CreatePen(0, 4, Rgb(0, 0, 139));
                        IntPtr oldPen = SelectObject(hdc, transformPen);
                        XFORM translation = new XFORM {
                            eM11 = 1.0f, eM22 = 1.0f, eDx = 45.0f, eDy = 315.0f
                        };
                        XFORM scale = new XFORM {
                            eM11 = 1.4f, eM22 = 0.7f
                        };
                        if (!SetWorldTransform(hdc, ref translation)
                            || !ModifyWorldTransform(hdc, ref scale, 2))
                            throw new InvalidOperationException("MWT_LEFTMULTIPLY failed");
                        MoveToEx(hdc, 0, 0, IntPtr.Zero); LineTo(hdc, 100, 0);
                        translation.eDx = 220.0f;
                        if (!SetWorldTransform(hdc, ref translation)
                            || !ModifyWorldTransform(hdc, ref scale, 3))
                            throw new InvalidOperationException("MWT_RIGHTMULTIPLY failed");
                        MoveToEx(hdc, 0, 0, IntPtr.Zero); LineTo(hdc, 100, 0);
                        SelectObject(hdc, oldPen); DeleteObject(transformPen);

                        XFORM bitmapTransform = new XFORM {
                            eM11 = 0.85f, eM12 = 0.35f, eM21 = -0.20f,
                            eM22 = 0.90f, eDx = 355.0f, eDy = 215.0f
                        };
                        if (!SetWorldTransform(hdc, ref bitmapTransform))
                            throw new InvalidOperationException("SetWorldTransform failed");
                        SetStretchBltMode(hdc, 3);
                        StretchDIBits(hdc, 0, 0, 150, 90, 0, 0, 3, 2, pixels, ref info, 0, 0x00CC0020);
                    }
                    finally { graphics.ReleaseHdc(hdc); }
                }
        });
    }

    static void GenerateLargeVector(string path)
    {
        WithWmf(path, dc => {
            MoveToEx(dc, 10, 10, IntPtr.Zero);
            for (int i = 0; i < 5000; i++) LineTo(dc, 10 + (i % 580), 10 + ((i * 37) % 380));
        });
    }

    public static void Render(string input, string output, int width, int height)
    {
        byte[] bytes = File.ReadAllBytes(input);
        int offset = bytes.Length >= 22 && BitConverter.ToUInt32(bytes, 0) == 0x9AC6CDD7u ? 22 : 0;
        string temporary = input;
        bool deleteTemporary = false;
        if (offset == 0) {
            temporary = Path.Combine(Path.GetTempPath(), "metafile-rs-reference-" + Guid.NewGuid().ToString("N") + ".wmf");
            File.WriteAllBytes(temporary, bytes);
            deleteTemporary = true;
        }
        using (var bitmap = new Bitmap(width, height, PixelFormat.Format32bppArgb))
        using (var graphics = Graphics.FromImage(bitmap))
        using (var metafile = new Metafile(temporary))
        {
            graphics.Clear(Color.White);
            graphics.DrawImage(metafile, new System.Drawing.Rectangle(0, 0, width, height));
            bitmap.Save(output, ImageFormat.Png);
        }
        if (deleteTemporary) File.Delete(temporary);
    }
}

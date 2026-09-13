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
    [DllImport("gdi32.dll")] static extern bool PolyPolygon(IntPtr hdc, POINT[] points, int[] counts, int polygons);
    [DllImport("gdi32.dll")] static extern int SetPolyFillMode(IntPtr hdc, int mode);
    [DllImport("gdi32.dll")] static extern bool TextOut(IntPtr hdc, int x, int y, string text, int length);
    [DllImport("gdi32.dll")] static extern uint SetTextAlign(IntPtr hdc, uint align);
    [DllImport("gdi32.dll")] static extern int SetStretchBltMode(IntPtr hdc, int mode);
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
        GenerateEnhanced(Path.Combine(directory, "windows-gdiplus-emfplus.emf"), EmfType.EmfPlusDual);
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

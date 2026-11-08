using System.IO;
using System.Reflection;
using System.Windows;
using System.Windows.Media;
using System.Xml.Linq;
using WPath = System.Windows.Shapes.Path;

namespace ManagerSetup;
// Use the exact Lucide SVG geometry already shipped with the manager, without a browser runtime.
internal static class Icons
{
    public static FrameworkElement Get(string name, double size = 20, string color = "#BDBDBD")
    {
        using var stream = Assembly.GetExecutingAssembly().GetManifestResourceStream($"Icons.{name}.svg") ?? throw new InvalidDataException(name);
        var svg = XDocument.Load(stream);
        var group = new DrawingGroup();
        var pen = new Pen((Brush)new BrushConverter().ConvertFromString(color)!, 1.75) { StartLineCap = PenLineCap.Round, EndLineCap = PenLineCap.Round, LineJoin = PenLineJoin.Round };
        foreach (var element in svg.Root!.Elements())
        {
            Geometry? geometry = element.Name.LocalName switch
            {
                "path" => Geometry.Parse(element.Attribute("d")!.Value),
                "circle" => new EllipseGeometry(new Point(Number(element, "cx"), Number(element, "cy")), Number(element, "r"), Number(element, "r")),
                "polyline" => Geometry.Parse("M" + element.Attribute("points")!.Value),
                "line" => new LineGeometry(new Point(Number(element,"x1"), Number(element,"y1")), new Point(Number(element,"x2"), Number(element,"y2"))),
                "rect" => new RectangleGeometry(new Rect(Number(element,"x"),Number(element,"y"),Number(element,"width"),Number(element,"height")),Number(element,"rx"),Number(element,"rx")),
                _ => null
            };
            if (geometry != null) group.Children.Add(new GeometryDrawing(null, pen, geometry));
        }
        group.Children.Add(new GeometryDrawing(Brushes.Transparent, null, new RectangleGeometry(new Rect(0, 0, 24, 24))));
        return new System.Windows.Controls.Image { Source = new DrawingImage(group), Width = size, Height = size };
    }
    private static double Number(XElement e, string n) => double.Parse(e.Attribute(n)?.Value ?? "0", System.Globalization.CultureInfo.InvariantCulture);
}

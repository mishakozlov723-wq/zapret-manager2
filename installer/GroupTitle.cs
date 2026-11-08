using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;

namespace ManagerSetup;
// .group-title: uppercase 12px label followed by a 1px divider line.
public class GroupTitle : DockPanel
{
    private readonly TextBlock label = new() { FontSize = 12, FontWeight = FontWeights.Bold, VerticalAlignment = VerticalAlignment.Center, TextWrapping = TextWrapping.NoWrap };
    public GroupTitle()
    {
        Margin = new Thickness(2, 24, 2, 10);
        label.Foreground = new SolidColorBrush(Color.FromArgb(0x8F, 255, 255, 255));
        Children.Add(label);
        Children.Add(new Border { Height = 1, Margin = new Thickness(10, 0, 0, 0), VerticalAlignment = VerticalAlignment.Center, Background = new SolidColorBrush(Color.FromArgb(0x13, 255, 255, 255)) });
    }
    public string Text { get => label.Text; set => label.Text = value.ToUpperInvariant(); }
}

using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;

namespace ManagerSetup;
// In-window folder picker, same structure and behaviour as the picker in ui/settings.js.
public partial class FolderPicker : UserControl
{
    public event Action<string>? Chosen;
    public event Action? Cancelled;
    /// <summary>Folder currently shown in the list (empty until a drive is opened).</summary>
    public string CurrentPath { get; private set; } = "";
    private bool loading;
    private int seq;

    public FolderPicker()
    {
        InitializeComponent();
        CloseX.Content = Icons.Get("x", 16, "#B3FFFFFF");
        CreateIcon.Content = Icons.Get("folder-plus", 16, "#EDEDED");
        TitleIcon.Content = MainWindow.Tile("folder-open", MainWindow.Neutral, 40, 20);
    }

    public async Task Open(string initial)
    {
        NewName.Text = ""; ShowError("");
        var folder = initial;
        while (!string.IsNullOrEmpty(folder) && !Directory.Exists(folder)) folder = Path.GetDirectoryName(folder) ?? "";
        if (string.IsNullOrEmpty(folder)) folder = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        await Navigate(folder);
        CloseX.Focus();
    }

    public bool IsReady => !loading;

    internal async Task Navigate(string path)
    {
        var my = ++seq; loading = true; ShowError("");
        Choose.IsEnabled = CreateButton.IsEnabled = false;
        try
        {
            if (!Path.IsPathFullyQualified(path) || path.StartsWith(@"\\")) throw new IOException("Выберите папку на локальном диске.");
            var canonical = Path.GetFullPath(path);
            if (new DriveInfo(Path.GetPathRoot(canonical)!).DriveType != DriveType.Fixed) throw new IOException("Выберите папку на локальном диске.");
            var dirs = await Task.Run(() => new DirectoryInfo(canonical).EnumerateDirectories()
                .Where(d => (d.Attributes & (FileAttributes.Hidden | FileAttributes.System | FileAttributes.ReparsePoint)) == 0)
                .Select(d => d.Name).OrderBy(n => n, StringComparer.CurrentCultureIgnoreCase).ToArray());
            if (my != seq) return;
            CurrentPath = canonical;
            Render(dirs);
        }
        catch (Exception ex) { if (my == seq) ShowError("Не удалось открыть папку. " + ex.Message); }
        finally
        {
            if (my == seq)
            {
                loading = false;
                var isRoot = CurrentPath.Length <= 3;
                Choose.IsEnabled = CurrentPath.Length > 0 && !isRoot;
                ChooseText.Text = isRoot ? "Откройте папку" : "Выбрать эту папку";
                CreateButton.IsEnabled = CurrentPath.Length > 0;
            }
        }
    }

    private void Render(string[] dirs)
    {
        Drives.Children.Clear();
        foreach (var d in DriveInfo.GetDrives().Where(d => d.IsReady && d.DriveType == DriveType.Fixed))
        {
            var letter = d.Name[..2];
            var active = CurrentPath.StartsWith(letter, StringComparison.OrdinalIgnoreCase);
            var b = new Button { Style = (Style)FindResource(active ? "DriveActive" : "Drive"), Tag = d.Name };
            b.Content = Row(Icons.Get("hard-drive", 14, active ? "#171717" : "#EDEDED"), letter, 6);
            b.Click += async (_, _) => await Navigate((string)b.Tag);
            Drives.Children.Add(b);
        }

        Crumbs.Children.Clear();
        var parts = CurrentPath.TrimEnd('\\').Split('\\');
        for (var i = 0; i < parts.Length; i++)
        {
            if (i > 0) Crumbs.Children.Add(new ContentControl { Content = Icons.Get("chevron-right", 14, "#85FFFFFF"), VerticalAlignment = VerticalAlignment.Center });
            var target = string.Join('\\', parts[..(i + 1)]) + (i == 0 ? "\\" : "");
            var c = new Button { Style = (Style)FindResource("Crumb"), Content = parts[i], Tag = target };
            if (i == parts.Length - 1) c.Foreground = (System.Windows.Media.Brush)FindResource("Tx");
            c.Click += async (_, _) => await Navigate((string)c.Tag);
            Crumbs.Children.Add(c);
        }

        Folders.Children.Clear();
        var style = (Style)FindResource("DirRow");
        var first = true;
        if (Directory.GetParent(CurrentPath) is { } parent)
        {
            var up = new Button { Style = style, Content = Row(Icons.Get("arrow-up", 16, "#9EFFFFFF"), "Наверх", 10), Tag = parent.FullName, BorderThickness = new Thickness(0) };
            up.Click += async (_, _) => await Navigate((string)up.Tag);
            Folders.Children.Add(up); first = false;
        }
        foreach (var n in dirs)
        {
            var full = Path.Combine(CurrentPath, n);
            var b = new Button { Style = style, Content = Row(Icons.Get("folder", 16, "#F0C062"), n, 10), Tag = full, BorderThickness = new Thickness(0, first ? 0 : 1, 0, 0) };
            b.Click += async (_, _) => await Navigate((string)b.Tag);
            Folders.Children.Add(b); first = false;
        }
        if (dirs.Length == 0)
            Folders.Children.Add(new TextBlock { Text = "Здесь нет папок. Можно создать новую ниже.", Foreground = (System.Windows.Media.Brush)FindResource("Tx3"), Margin = new Thickness(16) });
    }

    private static StackPanel Row(FrameworkElement icon, string text, double gap)
    {
        icon.Margin = new Thickness(0, 0, gap, 0); icon.VerticalAlignment = VerticalAlignment.Center;
        var p = new StackPanel { Orientation = Orientation.Horizontal };
        p.Children.Add(icon); p.Children.Add(new TextBlock { Text = text, VerticalAlignment = VerticalAlignment.Center, TextWrapping = TextWrapping.NoWrap });
        return p;
    }

    private void ShowError(string text) { FolderError.Text = text; FolderError.Visibility = text.Length > 0 ? Visibility.Visible : Visibility.Collapsed; }
    internal string ErrorMessage => FolderError.Text;

    private async void NewNameKey(object sender, KeyEventArgs e) { if (e.Key == Key.Enter) { e.Handled = true; await Create(); } }
    private async void CreateClick(object sender, RoutedEventArgs e) => await Create();
    internal async Task Create()
    {
        if (loading || CurrentPath.Length == 0) return;
        try
        {
            var name = NewName.Text.Trim();
            if (string.IsNullOrEmpty(name) || name.Length > 80 || name.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0 || name.EndsWith('.') || name is "." or "..") throw new IOException("Введите корректное название папки.");
            var next = InstallEngine.ValidatePath(Path.Combine(CurrentPath, name));
            if (Directory.Exists(next)) throw new IOException("Такая папка уже есть.");
            Directory.CreateDirectory(next); NewName.Text = ""; await Navigate(next);
        }
        catch (Exception ex) { ShowError(ex.Message); }
    }

    private void ChooseClick(object sender, RoutedEventArgs e) => TryChoose();
    internal bool TryChoose()
    {
        if (loading) return false;
        try { var p = InstallEngine.ValidatePath(CurrentPath); Chosen?.Invoke(p); return true; }
        catch (Exception ex) { ShowError(ex.Message); return false; }
    }
    private void CancelClick(object sender, RoutedEventArgs e) { ++seq; loading = false; Cancelled?.Invoke(); }
}

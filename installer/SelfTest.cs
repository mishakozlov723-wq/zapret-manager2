using System.IO;
using System.Reflection;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;

namespace ManagerSetup;
internal static class SelfTest
{
    // Safe desktop regression: never starts NSIS or modifies installed app/settings/services.
    public static void Run(string output)
    {
        Directory.CreateDirectory(output);
        var checks = new List<string>();
        void Assert(bool condition, string name) { if (!condition) throw new Exception("FAIL " + name); checks.Add("PASS " + name); }
        var root = Path.GetPathRoot(Path.GetFullPath(output))!;
        foreach (var bad in new[] { "", root, "C:", "relative", @"\\server\share", root + @"a\..\b", root + "NUL", root + @"a\CON.txt", root + "bad\"name", Environment.GetFolderPath(Environment.SpecialFolder.Windows) })
        {
            bool rejected = false; try { InstallEngine.ValidatePath(bad); } catch (IOException) { rejected = true; }
            Assert(rejected, "reject path: " + bad);
        }
        Assert(InstallEngine.ValidatePath(root + "Папка с пробелами\\Менеджер") == root + "Папка с пробелами\\Менеджер", "Cyrillic and spaces");
        using (var payload = Assembly.GetExecutingAssembly().GetManifestResourceStream("Payload.setup.exe"))
            Assert(payload != null && payload.Length > 100000 && payload.ReadByte() == 77 && payload.ReadByte() == 90, "real embedded NSIS executable");
        using (var size = Assembly.GetExecutingAssembly().GetManifestResourceStream("Payload.app.size"))
            Assert(size != null && long.TryParse(new StreamReader(size).ReadToEnd().Trim(), out var bytes) && bytes > 100000, "embedded app size for real progress");
        var families = Fonts.GetFontFamilies(new Uri("pack://application:,,,/Assets/Fonts/"));
        Assert(families.Any(f => f.FamilyNames.Values.Any(n => n.Contains("Manrope"))), "embedded Manrope");
        Assert(families.Any(f => f.FamilyNames.Values.Any(n => n.Contains("Poppins"))), "embedded Poppins");
        var manrope = families.First(f => f.FamilyNames.Values.Any(n => n.Contains("Manrope")));
        var weights = manrope.GetTypefaces().Select(t => t.Weight.ToOpenTypeWeight()).ToHashSet();
        Assert(new[] { 400, 500, 600, 700 }.All(weights.Contains), "Manrope has real Regular/Medium/SemiBold/Bold");
        // Behavioural check: text rendered with the UI font must measure exactly like the embedded fonts, not like Segoe UI.
        var ui = (FontFamily)Application.Current.FindResource("UiFont");
        var fontsBase = new Uri("pack://application:,,,/");
        var poppins = new FontFamily(fontsBase, "./Assets/Fonts/#Poppins");
        var manropeFace = new FontFamily(fontsBase, "./Assets/Fonts/#Manrope");
        var segoe = new FontFamily("Segoe UI");
        double W(FontFamily f, string s, FontWeight w) => new FormattedText(s, System.Globalization.CultureInfo.InvariantCulture, System.Windows.FlowDirection.LeftToRight,
            new Typeface(f, FontStyles.Normal, w, FontStretches.Normal), 20, System.Windows.Media.Brushes.White, 1.0).WidthIncludingTrailingWhitespace;
        // Strings without spaces: a space exists in both fonts, so mixed text is measured per script.
        foreach (var w in new[] { FontWeights.Normal, FontWeights.SemiBold, FontWeights.Bold })
        {
            Assert(Math.Abs(W(ui, "ZapretManager", w) - W(poppins, "ZapretManager", w)) < 0.01 && Math.Abs(W(ui, "ZapretManager", w) - W(segoe, "ZapretManager", w)) > 0.5,
                $"Latin renders in Poppins {w} (ui={W(ui, "ZapretManager", w):F2} poppins={W(poppins, "ZapretManager", w):F2} segoe={W(segoe, "ZapretManager", w):F2} manrope={W(manropeFace, "ZapretManager", w):F2})");
            Assert(Math.Abs(W(ui, "Установка0123456789", w) - W(manropeFace, "Установка0123456789", w)) < 0.01 && Math.Abs(W(ui, "Установка0123456789", w) - W(segoe, "Установка0123456789", w)) > 0.5,
                $"Cyrillic and digits render in Manrope {w} (ui={W(ui, "Установка0123456789", w):F2} manrope={W(manropeFace, "Установка0123456789", w):F2})");
        }

        var window = new MainWindow(); window.Show(); Pump(); Capture(window, output, "01-location");
        NativeCapture.Save(window, Path.Combine(output, "00-native-window.png"));
        window.Width = window.MinWidth; window.Height = window.MinHeight; Pump(); Capture(window, output, "07-minimum-size");
        window.Width = 980; window.Height = 660; Pump();
        Assert(window.FolderIcon.Content != null && window.Icon != null, "brand and folder icon");
        // Sharp text: Display mode snaps unhinted Manrope to pixels (jagged), Effects rasterise text (blurry);
        // ClearType (falls back to grayscale where Windows disables it) + gasp smoothing in the fonts.
        Assert(TextOptions.GetTextFormattingMode(window) == TextFormattingMode.Ideal && TextOptions.GetTextRenderingMode(window) == TextRenderingMode.ClearType, "text uses smooth Ideal/ClearType rendering");
        static IEnumerable<DependencyObject> Tree(DependencyObject d) { yield return d; for (var i = 0; i < VisualTreeHelper.GetChildrenCount(d); i++) foreach (var c in Tree(VisualTreeHelper.GetChild(d, i))) yield return c; }
        Assert(!Tree(window).OfType<UIElement>().Any(u => u.Effect != null && Tree(u).OfType<TextBlock>().Any()), "no effect rasterises text");

        window.InstallPath.Text = "relative";
        window.ActionButton.RaiseEvent(new RoutedEventArgs(Button.ClickEvent)); Pump();
        Assert(window.ErrorBox.Visibility == Visibility.Visible && window.ErrorText.Text.Length > 0 && window.LocationPage.Visibility == Visibility.Visible, "invalid install path shows recoverable error");
        Capture(window, output, "02-error");
        window.InstallPath.Text = InstallEngine.DefaultPath; Pump();
        Assert(window.ErrorBox.Visibility == Visibility.Collapsed, "error hides after path edit");

        // In-window picker (same as manager settings), not a separate system window.
        var open = window.OpenPicker(); Wait(() => open.IsCompleted);
        Assert(window.PickerLayer.Visibility == Visibility.Visible, "picker opens inside installer window");
        var nav = window.Picker.Navigate(root); Wait(() => nav.IsCompleted); Pump();
        Capture(window, output, "03-folders");
        Assert(window.Picker.CurrentPath == root, "drive root can be browsed");
        Assert(!window.Picker.Choose.IsEnabled, "drive root cannot be installation destination");
        var full = Path.GetFullPath(output);
        nav = window.Picker.Navigate(full); Wait(() => nav.IsCompleted);
        Assert(window.Picker.CurrentPath == full && window.Picker.Choose.IsEnabled, "navigate directory");
        var testFolder = Path.Combine(full, "Проверка папки " + Guid.NewGuid().ToString("N"));
        window.Picker.NewName.Text = Path.GetFileName(testFolder);
        var create = window.Picker.Create(); Wait(() => create.IsCompleted);
        Assert(Directory.Exists(testFolder) && window.Picker.CurrentPath == testFolder, "create and enter folder through UI");
        Assert(window.Picker.TryChoose() && window.InstallPath.Text == testFolder && window.PickerLayer.Visibility == Visibility.Collapsed, "choose folder fills install path and closes picker");
        Directory.Delete(testFolder);
        window.InstallPath.Text = InstallEngine.DefaultPath; Pump();

        var fg = ((window.ActionButton.Content as Panel)!.Children[1] as TextBlock)!.Foreground as SolidColorBrush;
        Assert(fg != null && fg.Color == Color.FromRgb(23, 23, 23), "primary button dark text");

        // Progress: bar follows reported fraction and never jumps.
        window.BeginProgress(); Pump();
        window.Report(new InstallProgress(0.42, "Записываем файлы программы…"));
        Wait(() => window.Progress.Value >= 0.42 - 1e-6);
        Assert(window.PercentText.Text == "42%" && window.ProgressTitle.Text == "Записываем программу", "bar and title follow real progress");
        Assert(window.ProgressPage.Visibility == Visibility.Visible && window.ContinueButton.Visibility != Visibility.Visible, "installation controls locked");
        Capture(window, output, "04-progress");
        window.Close(); Pump();
        Assert(window.IsVisible, "window cannot be closed while installing");

        // Instant install: engine is done at once, UI still fills smoothly and then waits.
        window.MarkCompleted(); Pump(); Thread.Sleep(150); Pump();
        Assert(window.Progress.Value < 0.99 && window.ContinueButton.Visibility != Visibility.Visible, "instant install does not jump to 100%");
        Wait(() => window.ContinueButton.Visibility == Visibility.Visible);
        Assert(window.PercentText.Text == "100%" && window.ProgressTitle.Text == "Установка завершена", "bar reaches 100%");
        for (var i = 0; i < 10; i++) { Pump(); Thread.Sleep(50); }
        Assert(window.ProgressPage.Visibility == Visibility.Visible && window.DonePage.Visibility != Visibility.Visible, "stays on progress page until Continue");
        Capture(window, output, "05-finished");
        window.ContinueButton.RaiseEvent(new RoutedEventArgs(Button.ClickEvent)); Pump();
        Assert(window.DonePage.Visibility == Visibility.Visible && window.OpenButton.IsEnabled, "Continue opens completion page with launch action");
        Capture(window, output, "06-complete");
        window.Close();
        File.WriteAllLines(Path.Combine(output, "results.txt"), checks.Append($"{checks.Count} checks passed; installation itself was NOT executed."));
    }
    static void Pump() => Application.Current.Dispatcher.Invoke(() => { }, DispatcherPriority.ApplicationIdle);
    static void Wait(Func<bool> done) { var start = DateTime.UtcNow; while (!done()) { Pump(); Thread.Sleep(10); if ((DateTime.UtcNow - start).TotalSeconds > 15) throw new TimeoutException(); } Pump(); }
    static void Capture(Window window, string output, string name)
    {
        window.UpdateLayout();
        var image = new RenderTargetBitmap((int)window.ActualWidth, (int)window.ActualHeight, 96, 96, PixelFormats.Pbgra32);
        image.Render(window); var encoder = new PngBitmapEncoder(); encoder.Frames.Add(BitmapFrame.Create(image));
        using var file = File.Create(Path.Combine(output, name + ".png")); encoder.Save(file);
    }
}

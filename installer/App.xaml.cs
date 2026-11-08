using System.Windows;

namespace ManagerSetup;
public partial class App : Application
{
    private Mutex? instance;
    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        // UI font: per-character fallback chain. The embedded Poppins has no digits, so Latin -> Poppins,
        // Cyrillic and digits -> Manrope (WPF ignored the .CompositeFont maps, text fell back to Segoe UI).
        Resources["UiFont"] = new System.Windows.Media.FontFamily(new Uri("pack://application:,,,/"), "./Assets/Fonts/#Poppins, ./Assets/Fonts/#Manrope");
        if (e.Args.Length == 2 && e.Args[0] == "--self-test")
        {
            try { SelfTest.Run(e.Args[1]); Shutdown(0); }
            catch (Exception ex) { System.IO.File.WriteAllText(System.IO.Path.Combine(e.Args[1], "failure.txt"), ex.ToString()); Shutdown(1); }
            return;
        }
        instance = new Mutex(true, @"Local\ZapretManagerSetup", out var first);
        if (!first) { Shutdown(0); return; }
        new MainWindow().Show();
    }
}

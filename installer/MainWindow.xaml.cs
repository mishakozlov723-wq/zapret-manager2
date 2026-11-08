using System.ComponentModel;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Shapes;
using System.Windows.Threading;

namespace ManagerSetup;
public partial class MainWindow : Window
{
    internal static readonly Color Neutral = C("#EDEDED");
    static readonly Color OkC = C("#5FCF98"), BadC = C("#FF7A6B"), InfoC = C("#7FB4F0"), IdleC = C("#707070");
    // Display stages; End is the overall fraction at which the stage is finished (matches InstallEngine).
    static readonly (string Title, string Detail, double End)[] Stages =
    {
        ("Распаковываем пакет", "Достаём установочные файлы из этого exe.", InstallEngine.UnpackEnd),
        ("Записываем программу", "Копируем Zapret Manager в выбранную папку.", InstallEngine.InstallEnd),
        ("Проверяем файлы", "Сверяем контрольную сумму с пакетом.", InstallEngine.VerifyEnd),
        ("Регистрируем в Windows", "Ярлыки и пункт для удаления в «Параметрах».", 1.0),
    };
    /// <summary>Fastest visible fill, fraction per second: an instant install still takes ~2 s to fill, never jumps.</summary>
    internal const double FillRate = 0.45;

    private bool busy, completed, finishedShown;
    private double target, shown;
    private readonly List<(double At, string Note)> notes = new();
    private readonly DispatcherTimer animator = new(DispatcherPriority.Render) { Interval = TimeSpan.FromMilliseconds(16) };
    private DateTime lastTick;
    private readonly List<(Border Card, Border Num, string Label)> stageCards = new();
    private readonly List<(Border Row, string Icon, string Label)> navRows = new();

    public MainWindow()
    {
        InitializeComponent();
        NativeChrome.Apply(this);
        InstallPath.Text = InstallEngine.DefaultPath;
        FolderIcon.Content = Icons.Get("folder-open", 16, "#EDEDED");
        RowIcon.Content = Tile("folder", Neutral, 34, 17);
        InstallIcon.Content = Icons.Get("download", 16, "#171717");
        ContinueIcon.Content = Icons.Get("chevron-right", 16, "#171717");
        OpenIcon.Content = Icons.Get("power", 16, "#171717");
        DoneIcon.Content = Tile("circle-check", OkC, 48, 24);

        foreach (var (icon, label) in new[] { ("folder-open", "Расположение"), ("download", "Установка"), ("circle-check", "Готово") })
        {
            var row = new Border { Height = 38, CornerRadius = new CornerRadius(10), Padding = new Thickness(10, 0, 10, 0), Margin = new Thickness(0, 0, 0, 3) };
            Nav.Children.Add(row); navRows.Add((row, icon, label));
        }
        SetNav(0);

        foreach (var (icon, text) in new[] { ("shield-check", "Без прав администратора"), ("settings", "Настройки сохранятся"), ("trash-2", "Удаляется в «Параметрах»") })
            Facts.Children.Add(Fact(icon, text));

        var planIcons = new[] { "folder-plus", "layout-grid", "monitor-check" };
        var plan = new[]
        {
            ("Скопируем программу", "Один файл менеджера и деинсталлятор — в папку выше."),
            ("Добавим ярлыки", "На рабочий стол и в меню «Пуск»."),
            ("Зарегистрируем в Windows", "Удалить можно в «Параметры → Приложения». Ваши настройки и служба Zapret сохранятся."),
        };
        for (var i = 0; i < plan.Length; i++) PlanSteps.Children.Add(StepCard((i + 1).ToString(), plan[i].Item1, plan[i].Item2, out _, planIcons[i]));
        for (var i = 0; i < Stages.Length; i++) { var card = StepCard((i + 1).ToString(), Stages[i].Title, Stages[i].Detail, out var num); StageList.Children.Add(card); stageCards.Add((card, num, (i + 1).ToString())); }
        var nextIcons = new[] { "power", "layout-grid", "trash-2" };
        var next = new[]
        {
            ("Откройте менеджер", "Начните с «Проверки компьютера» на главной — дальше программа проведёт по шагам."),
            ("Ярлыки уже на месте", "Zapret Manager есть на рабочем столе и в меню «Пуск»."),
            ("Удаление", "«Параметры Windows → Приложения → Zapret Manager». Настройки и служба Zapret не пострадают."),
        };
        for (var i = 0; i < next.Length; i++) NextSteps.Children.Add(StepCard((i + 1).ToString(), next[i].Item1, next[i].Item2, out _, nextIcons[i]));

        Hero(LocationHero, IdleC); LocationDot.Content = Tile("hard-drive", Neutral, 48, 24);
        Hero(DoneHero, OkC);
        Alert(ErrorBox, BadC); ErrorDot.Content = Icons.Get("triangle-alert", 18, "#FF7A6B");
        Alert(LaunchErrorBox, BadC);
        SetSide("Готов к установке", "Выберите папку", IdleC, false);
        if (InstallEngine.PreviousPath() != null)
        {
            LocationTitle.Text = "Обновим установленный менеджер"; LocationDot.Content = Tile("refresh-cw", InfoC, 48, 24);
            InstallNote.Text = "В прежней папке. Личные настройки, домены и работающая служба сохранятся.";
            BrowseButton.IsEnabled = false; ActionText.Text = "Обновить";
        }
        UpdateChip();

        Picker.Chosen += path => { InstallPath.Text = path; ClosePicker(); };
        Picker.Cancelled += ClosePicker;
        PickerLayer.MouseDown += (_, e) => { if (e.OriginalSource == PickerLayer) ClosePicker(); };
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape && PickerLayer.Visibility == Visibility.Visible) { e.Handled = true; ClosePicker(); } };
        animator.Tick += (_, _) => Tick();
        Closing += OnClosing;
    }

    // ---------- location
    private void OnClosing(object? sender, CancelEventArgs e)
    {
        if (busy) { e.Cancel = true; ProgressSubtitle.Text = "Дождитесь завершения установки — сейчас записываются файлы."; }
    }
    private void PathChanged(object sender, TextChangedEventArgs e) { if (ErrorBox != null) { HideError(); UpdateChip(); } }
    private void UpdateChip() => DefaultChip.Visibility = string.Equals(InstallPath.Text.TrimEnd('\\'), InstallEngine.DefaultPath.TrimEnd('\\'), StringComparison.OrdinalIgnoreCase) ? Visibility.Visible : Visibility.Collapsed;
    private void ShowError(string text) { ErrorText.Text = text; ErrorBox.Visibility = Visibility.Visible; }
    private void HideError() { ErrorText.Text = ""; ErrorBox.Visibility = Visibility.Collapsed; }

    private async void BrowseClick(object sender, RoutedEventArgs e) => await OpenPicker();
    internal async Task OpenPicker() { PickerLayer.Visibility = Visibility.Visible; await Picker.Open(InstallPath.Text); }
    private void ClosePicker() { PickerLayer.Visibility = Visibility.Collapsed; BrowseButton.Focus(); }
    private void CancelClick(object sender, RoutedEventArgs e) => Close();

    private async void ActionClick(object sender, RoutedEventArgs e)
    {
        HideError();
        string path;
        try { path = InstallEngine.ValidatePath(InstallPath.Text); InstallPath.Text = path; InstallEngine.CheckReady(path); }
        catch (Exception ex) { ShowError(ex.Message); return; }
        BeginProgress();
        try { await InstallEngine.Install(path, new Progress<InstallProgress>(Report)); MarkCompleted(); }
        catch (Exception ex) { FailProgress(ex.Message); }
    }

    // ---------- progress
    internal void BeginProgress()
    {
        busy = true; completed = finishedShown = false; target = shown = 0; notes.Clear();
        LocationPage.Visibility = Visibility.Collapsed; DonePage.Visibility = Visibility.Collapsed; ProgressPage.Visibility = Visibility.Visible;
        ContinueButton.Visibility = Visibility.Collapsed; PercentText.Visibility = Visibility.Visible;
        ProgressSubtitle.Text = "Не закрывайте окно — это займёт меньше минуты.";
        Hero(ProgressHero, InfoC); ProgressDot.Content = Tile("download", InfoC, 48, 24, blink: true);
        Progress.Foreground = new SolidColorBrush(InfoC);
        SetNav(1); SetSide("Устанавливаем…", "0%", InfoC, true);
        lastTick = DateTime.UtcNow; animator.Start(); Render();
    }
    /// <summary>Real progress from the engine; the bar never moves backwards.</summary>
    internal void Report(InstallProgress p)
    {
        var f = Math.Clamp(p.Fraction, 0, 1);
        target = Math.Max(target, f);
        if (!string.IsNullOrEmpty(p.Note)) notes.Add((f, p.Note));
    }
    internal void MarkCompleted() { target = 1; completed = true; }

    private void Tick()
    {
        var now = DateTime.UtcNow;
        var dt = Math.Min(0.1, (now - lastTick).TotalSeconds); lastTick = now;
        shown = Math.Min(target, shown + FillRate * dt);
        Render();
        if (completed && shown >= 1 && !finishedShown) ShowFinished();
    }

    private void Render()
    {
        Progress.Value = shown;
        var percent = (int)Math.Floor(shown * 100 + 1e-9);
        PercentText.Text = percent + "%";
        var idx = Array.FindIndex(Stages, s => shown < s.End - 1e-9);
        if (idx < 0) idx = Stages.Length - 1;
        ProgressTitle.Text = Stages[idx].Title;
        // Only show an engine note that belongs to what the bar currently displays.
        var low = idx == 0 ? 0 : Stages[idx - 1].End;
        var note = notes.LastOrDefault(n => n.At <= shown + 1e-9 && n.At >= low - 1e-9).Note;
        ProgressText.Text = string.IsNullOrEmpty(note) ? Stages[idx].Detail : note;
        for (var i = 0; i < stageCards.Count; i++) SetStep(stageCards[i].Card, stageCards[i].Num, stageCards[i].Label, i < idx || shown >= 1 ? 2 : i == idx ? 1 : 0);
        SideDetail.Text = percent + "% · " + Stages[idx].Title.ToLowerInvariant();
    }

    private void ShowFinished()
    {
        finishedShown = true; busy = false; animator.Stop();
        shown = 1; Render();
        ProgressTitle.Text = "Установка завершена";
        ProgressText.Text = "Все файлы на месте и проверены. Нажмите «Продолжить».";
        ProgressSubtitle.Text = "Готово — можно переходить дальше.";
        Hero(ProgressHero, OkC); ProgressDot.Content = Tile("circle-check", OkC, 48, 24);
        Progress.Foreground = new SolidColorBrush(OkC);
        ContinueButton.Visibility = Visibility.Visible; ContinueButton.Focus();
        SetSide("Установлено", "100%", OkC, false);
    }

    private void FailProgress(string message)
    {
        busy = false; animator.Stop();
        ProgressPage.Visibility = Visibility.Collapsed; LocationPage.Visibility = Visibility.Visible;
        ShowError(message); SetNav(0); SetSide("Ошибка установки", "Можно повторить", BadC, false);
    }

    private void ContinueClick(object sender, RoutedEventArgs e) => ShowDone();
    internal void ShowDone()
    {
        ProgressPage.Visibility = Visibility.Collapsed; DonePage.Visibility = Visibility.Visible;
        DonePath.Text = InstallPath.Text; SetNav(2, allDone: true);
        SetSide("Установлено", "Можно открыть менеджер", OkC, false);
        OpenButton.Focus();
    }
    private void OpenClick(object sender, RoutedEventArgs e)
    {
        try { InstallEngine.Launch(InstallPath.Text); Close(); }
        catch (Exception ex) { LaunchErrorText.Text = "Не удалось открыть программу: " + ex.Message; LaunchErrorBox.Visibility = Visibility.Visible; }
    }

    // ---------- visual helpers (tokens from ui/style.css)
    private void SetNav(int active, bool allDone = false)
    {
        for (var i = 0; i < navRows.Count; i++)
        {
            var (row, icon, label) = navRows[i];
            var done = i < active || (allDone && i == active);
            var current = i == active;
            row.Background = current ? new SolidColorBrush(Color.FromArgb(0x16, 255, 255, 255)) : Brushes.Transparent;
            var p = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
            var ic = done ? Icons.Get("check", 16, "#5FCF98") : Icons.Get(icon, 16, current ? "#EDEDED" : "#9EFFFFFF");
            ic.Margin = new Thickness(0, 0, 10, 0); p.Children.Add(ic);
            p.Children.Add(new TextBlock { Text = label, FontWeight = FontWeights.SemiBold, FontSize = 13.5, VerticalAlignment = VerticalAlignment.Center, Foreground = current ? (Brush)FindResource("Tx") : (Brush)FindResource("Tx2") });
            row.Child = p;
        }
    }

    private void SetSide(string title, string detail, Color tone, bool blink) { SideTitle.Text = title; SideDetail.Text = detail; SideDot.Content = Dot(tone, 10, blink); }

    private Border StepCard(string number, string title, string detail, out Border num, string? icon = null)
    {
        num = new Border { Width = 30, Height = 30, CornerRadius = new CornerRadius(15), VerticalAlignment = VerticalAlignment.Top, Margin = new Thickness(0, 0, 14, 0) };
        var body = new StackPanel();
        body.Children.Add(new TextBlock { Text = title, FontWeight = FontWeights.Bold, FontSize = 14 });
        body.Children.Add(new TextBlock { Text = detail, FontSize = 13, Foreground = (Brush)FindResource("Tx2"), Margin = new Thickness(0, 2, 0, 0) });
        var dock = new DockPanel(); dock.Children.Add(num);
        if (icon != null) { var ic = Icons.Get(icon, 18, "#85FFFFFF"); ic.VerticalAlignment = VerticalAlignment.Center; ic.Margin = new Thickness(14, 0, 0, 0); DockPanel.SetDock(ic, Dock.Right); dock.Children.Add(ic); }
        dock.Children.Add(body);
        var card = new Border { CornerRadius = new CornerRadius(14), BorderThickness = new Thickness(1), Padding = new Thickness(16, 15, 18, 15), Margin = new Thickness(0, 0, 0, 8), Child = dock };
        SetStep(card, num, number, 1, plain: true);
        return card;
    }

    /// <summary>state: 0 locked, 1 current, 2 done (.step, .step.current, .step.done).</summary>
    private void SetStep(Border card, Border num, string label, int state, bool plain = false)
    {
        card.Background = (Brush)FindResource(state == 1 && !plain ? "Card2" : "Card");
        card.BorderBrush = (Brush)FindResource(state == 1 && !plain ? "Ln2" : "Ln");
        card.Opacity = state == 0 ? 0.5 : 1;
        if (state == 2)
        {
            num.Background = new SolidColorBrush(Color.FromArgb(0x38, OkC.R, OkC.G, OkC.B));
            if (num.Tag as string != "done") { num.Child = Icons.Get("check", 15, "#5FCF98"); ((FrameworkElement)num.Child).HorizontalAlignment = HorizontalAlignment.Center; num.Tag = "done"; }
            return;
        }
        var current = state == 1 && !plain;
        num.Background = current ? (Brush)FindResource("Pri") : new SolidColorBrush(Color.FromArgb(0x14, 255, 255, 255));
        var key = current ? "cur" : "idle";
        if (num.Tag as string != key)
        {
            num.Child = new TextBlock { Text = label, FontWeight = FontWeights.Bold, FontSize = 13, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center, Foreground = current ? (Brush)FindResource("PriTx") : (Brush)FindResource("Tx2") };
            num.Tag = key;
        }
    }

    /// <summary>.hero / .statusbar: 135° gradient tinted by the tone, tone border at 30%.</summary>
    private static void Hero(Border b, Color tone)
    {
        b.Background = new LinearGradientBrush(Mix(tone, C("#232323"), 0.10), C("#1D1D1D"), new Point(0, 0), new Point(1, 1));
        b.BorderBrush = new SolidColorBrush(Color.FromArgb(77, tone.R, tone.G, tone.B));
    }
    /// <summary>.alert: tone 8% over card, border 28%.</summary>
    private static void Alert(Border b, Color tone)
    {
        b.Background = new SolidColorBrush(Mix(tone, C("#212121"), 0.08));
        b.BorderBrush = new SolidColorBrush(Color.FromArgb(71, tone.R, tone.G, tone.B));
    }
    /// <summary>.dot: filled circle with an 18% halo ring; blinks while busy (off with reduced motion).</summary>
    private static FrameworkElement Dot(Color tone, double size, bool blink)
    {
        var g = new Grid { Width = size + 8, Height = size + 8 };
        g.Children.Add(new Ellipse { Fill = new SolidColorBrush(Color.FromArgb(46, tone.R, tone.G, tone.B)) });
        g.Children.Add(new Ellipse { Width = size, Height = size, Fill = new SolidColorBrush(tone) });
        if (blink && SystemParameters.ClientAreaAnimation)
            g.BeginAnimation(OpacityProperty, new DoubleAnimation(1, 0.3, TimeSpan.FromMilliseconds(450)) { AutoReverse = true, RepeatBehavior = RepeatBehavior.Forever, EasingFunction = new SineEase() });
        return g;
    }
    /// <summary>Rounded icon tile (like the manager's card icons): tone at 14% with a 26% border, icon in the tone.</summary>
    internal static FrameworkElement Tile(string icon, Color tone, double size, double iconSize, bool blink = false)
    {
        var ic = Icons.Get(icon, iconSize, tone.ToString()); ic.HorizontalAlignment = HorizontalAlignment.Center; ic.VerticalAlignment = VerticalAlignment.Center;
        var tile = new Border { Width = size, Height = size, CornerRadius = new CornerRadius(size * 0.3), Background = new SolidColorBrush(Color.FromArgb(0x24, tone.R, tone.G, tone.B)),
            BorderBrush = new SolidColorBrush(Color.FromArgb(0x42, tone.R, tone.G, tone.B)), BorderThickness = new Thickness(1), Child = ic };
        // Blink only the icon: opacity on the whole tile would drop the hero text out of ClearType.
        if (blink && SystemParameters.ClientAreaAnimation)
            ic.BeginAnimation(OpacityProperty, new DoubleAnimation(1, 0.35, TimeSpan.FromMilliseconds(600)) { AutoReverse = true, RepeatBehavior = RepeatBehavior.Forever, EasingFunction = new SineEase() });
        return tile;
    }
    /// <summary>Small info chip under the folder card.</summary>
    private FrameworkElement Fact(string icon, string text)
    {
        var p = new StackPanel { Orientation = Orientation.Horizontal };
        var ic = Icons.Get(icon, 15, "#9EFFFFFF"); ic.Margin = new Thickness(0, 0, 7, 0); ic.VerticalAlignment = VerticalAlignment.Center; p.Children.Add(ic);
        p.Children.Add(new TextBlock { Text = text, FontSize = 12.5, FontWeight = FontWeights.SemiBold, Foreground = (Brush)FindResource("Tx2"), VerticalAlignment = VerticalAlignment.Center, TextWrapping = TextWrapping.NoWrap });
        return new Border { Background = (Brush)FindResource("Card"), BorderBrush = (Brush)FindResource("Ln"), BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(10), Padding = new Thickness(11, 7, 12, 7), Margin = new Thickness(0, 0, 8, 8), Child = p };
    }
    private static Color C(string hex) => (Color)ColorConverter.ConvertFromString(hex);
    private static Color Mix(Color a, Color b, double t) => Color.FromRgb((byte)(a.R * t + b.R * (1 - t)), (byte)(a.G * t + b.G * (1 - t)), (byte)(a.B * t + b.B * (1 - t)));
}

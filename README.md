# Photo Widget

![Photo Widget Screenshot](icon.png)
`
*Read this in [简体中文](README-zh.md)* 

This is a lightweight desktop widget designed to display random images on your desktop. It aims to provide an aesthetic and unobtrusive way to showcase your photos, supporting custom image folders, refresh intervals, and window sizing.

## ✨ Features

*   **Custom Image Folders**: Add multiple folders containing your favorite image collections.
*   **Random Image Display**: Automatically selects and displays images randomly from the specified folders.
*   **Configurable Refresh Interval**: Set the time interval (seconds, minutes, hours) for automatic image switching, or disable automatic refresh.
*   **Smart Image Loading**:
    *   Optimizes image display based on original dimensions and your configuration, supporting `Cover` (fill and crop) and `Contain` (fit within window and resize window) modes.
    *   Can filter images by landscape or portrait orientation.
*   **Flexible Window Control**:
    *   **Borderless Transparent Window**: Blends into the desktop and does not occupy space on the taskbar.
    *   **"Always on Top" Option**: Ensures the photo widget is always visible.
    *   **Multi-Anchor Resizing**: When the window size changes, you can choose to keep the window's center, top-left, top-right, bottom-left, or bottom-right position fixed.
    *   **Automatic Screen Boundary Check**: Prevents the window from getting lost off-screen and automatically moves it back into view.
    *   **Drag Bar**: A control bar appears on hover, allowing you to drag the window.
    *   **Click to Switch**: Left-click the image to switch to the next one.
    *   **Context Menu**: Right-click the image or use the system tray icon to quickly access settings.
*   **Tray Icon Integration**: Provides a system tray icon for easy access to settings and application exit.
*   **Persistent Configuration**: Automatically saves your settings to a local file.

## 🚀 How to Run

1.  **Download Executable**: Visit [Releases](https://github.com/Kevin589981/desktop-widget/releases) to download the latest `photo_widget.exe` (or the corresponding executable for your operating system).
2.  **Run**: Double-click the downloaded executable to start the application. It will appear as a borderless window on your desktop and display an icon in the system tray.
3.  **Initial Setup**: When running for the first time, you may need to right-click the image or the system tray icon and select "Settings" to add your image folders.

## 🛠️ How to Build

This project is developed using the Rust programming language and the `eframe` GUI framework.

**Prerequisites**:

*   Install [Rust](https://www.rust-lang.org/tools/install).

**Building Steps**:

1.  **Clone the Repository**:
    ```bash
    git clone https://github.com/Kevin589981/desktop-widget.git
    cd desktop-widget
    ```

2.  **Build**:
    ```bash
    cargo build --release
    ```
    This will generate the executable file in the `target/release/` directory.

## ⚙️ Configuration File

Project settings are saved in a file named `photo_widget_config.json`, located in the same directory as the application executable.
You can manually edit this file, but it's generally recommended to modify settings through the application's user interface.

**Example Configuration (photo_widget_config.json):**

```json
{
  "folders": [
    "C:\\Users\\YourUser\\Pictures",
    "/home/youruser/Images"
  ],
  "always_on_top": true,
  "refresh_interval": 300,
  "refresh_value": 5,
  "refresh_unit": "Minutes",
  "landscape_width": 400.0,
  "landscape_height": 300.0,
  "portrait_width": 300.0,
  "portrait_height": 400.0,
  "fit_mode": "Cover",
  "resize_anchor": "Center",
  "orientation_filter": "Both"
}
```

## 📄 License

This project is licensed under the **Apache License, Version 2.0**. You can find the full license text in the [LICENSE](LICENSE) file.

**Apache License Summary:**

You are free to use, modify, and distribute this software for both commercial and non-commercial purposes. However, you must:

*   Retain all copyright, patent, trademark, and attribution notices.
*   State any changes you made in any modified files.
*   Include a copy of the license and a NOTICE file (if one exists) when distributing the software.

---
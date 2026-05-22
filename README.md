# Chrome Debloat

A TUI tool for automatically configuring and applying policies to Chromium-based browsers, ensuring a cleaner browsing experience.

Instantly disable telemetry, promotional clutter, and browser bloat while maintaining full usability. Includes a built-in editor for fine-tuning policies to your needs.

> [!WARNING]
> The old policy generation script is available in `legacy` branch, but it will not be maintained.

## Features

- Disable telemetry and usage reporting
- Disable GenAI features
- Disable bloatware, promotional features and suggested content
- Disable vendor specific Sign-In and Sync
- Tighten Site-Shield settings (disable location detection, notifications, etc)
- Install content-blocking extensions:
  - **Brave**
    - uBlock Origin
    - I still don't care about cookies
  - **Chrome**
    - uBlock Origin Lite
    - I still don't care about cookies
  - **Edge** (from edge addons store)
    - uBlock Origin
    - I still don't care about cookies
    - Blank Tab

## Supported Browsers

| Browser | Windows | macOS | Linux |
|---------|---------|-------|-------|
| Google Chrome | ✅ | ✅ | ✅ |
| Microsoft Edge | ✅ | ✅ | ✅ |
| Brave | ✅ | ✅ | ✅ |

## Quick Start

This tool does not require installation, and can be run in a single command.

### Linux / macOS

```bash
curl https://debloat.yashg.dev/install.sh | sh
```

### Windows

```powershell
irm https://debloat.yashg.dev/install.ps1 | iex
```

Or download the binary from the [latest GitHub release](https://github.com/yashgorana/chrome-debloat/releases/latest) and run it directly.

## Policy Documentation

- [Chrome Enterprise Policies](https://chromeenterprise.google/policies/)
- [Brave Policies](https://support.brave.com/hc/en-us/articles/360039248271-Group-Policy)
- [Microsoft Edge Policies](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-policies)

## License

[Apache 2.0](./LICENSE)

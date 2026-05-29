Teletipo installers
==================

Linux/macOS:
  sh ./install.sh
  sh ./install.sh --desktop
  sh ./uninstall.sh

Windows PowerShell:
  powershell -ExecutionPolicy Bypass -File .\install.ps1
  powershell -ExecutionPolicy Bypass -File .\uninstall.ps1

Standalone quick install:
  curl -fsSL https://github.com/fernandoescolar/teletipo/releases/latest/download/install.sh | sh
  irm https://github.com/fernandoescolar/teletipo/releases/latest/download/install.ps1 | iex

; Per-user Windows installer for the staged package. Built by
; `scripts/dev.ps1 package`, which passes the defines below; do not run by hand.
#ifndef StageDir
  #error Build this installer with scripts/dev.ps1 package
#endif

[Setup]
; AppId ties upgrades and uninstall together. Never change it.
AppId={{6E337340-B44D-4C58-A31D-C5CF1C8CA642}
AppName=fuckMail
AppVersion={#AppVersion}
AppVerName=fuckMail {#AppVersion}
VersionInfoVersion={#NumericVersion}
; Installs for the current user only, under %LOCALAPPDATA%\Programs: no admin prompt.
PrivilegesRequired=lowest
DefaultDirName={autopf}\fuckMail
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\bin\fuckmail-gui.exe
UninstallDisplayName=fuckMail
; Ask to close a running GUI, CLI or engine before replacing their files.
CloseApplications=yes
RestartApplications=no
WizardStyle=modern
SetupIconFile=..\gui\app-icon\app-icon.ico
WizardSmallImageFile=..\gui\app-icon\app-icon.png
Compression=lzma2
SolidCompression=yes

[Tasks]
Name: desktopicon; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked

[InstallDelete]
; An upgrade replaces the previous build's program files completely, so no
; stale Qt library or QML file survives. Mail data lives in %LOCALAPPDATA%\fuckMail.
Type: filesandordirs; Name: "{app}\bin"
Type: filesandordirs; Name: "{app}\plugins"
Type: filesandordirs; Name: "{app}\qml"
Type: filesandordirs; Name: "{app}\translations"

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{autoprograms}\fuckMail"; Filename: "{app}\bin\fuckmail-gui.exe"
Name: "{autodesktop}\fuckMail"; Filename: "{app}\bin\fuckmail-gui.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\bin\fuckmail-gui.exe"; Description: "{cm:LaunchProgram,fuckMail}"; Flags: nowait postinstall skipifsilent

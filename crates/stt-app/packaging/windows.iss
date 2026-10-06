#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef InstallerAppId
  #define InstallerAppId "{{5B67D92C-AF30-4B23-95C3-7D671DFA008B}"
#endif
#ifndef AppGroupName
  #define AppGroupName "Plume"
#endif

[Setup]
AppId={#InstallerAppId}
AppName=Plume
AppVersion={#AppVersion}
AppPublisher=Leo Martin
DefaultDirName={localappdata}\Programs\Plume
DefaultGroupName={#AppGroupName}
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
SetupIconFile=..\assets\brand\plume.ico
UninstallDisplayIcon={app}\plume.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
DisableProgramGroupPage=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Plume"; Filename: "{app}\plume.exe"
Name: "{autodesktop}\Plume"; Filename: "{app}\plume.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\plume.exe"; Description: "Launch Plume"; Flags: nowait postinstall skipifsilent

; Unsigned Windows 11 x64 development installer. Build through the PowerShell wrapper.
#ifndef PayloadDirectory
  #error PayloadDirectory is required
#endif
#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef SourceRevision
  #error SourceRevision is required
#endif
#if Len(PayloadDirectory) == 0
  #error Empty payload directory is forbidden
#endif
#if !FileExists(PayloadDirectory + "\INSTALLER-IDENTITY.ini")
  #error The payload must include its generated installer identity
#endif
#define KlickyAppId "{B8E55779-4EDB-4D8F-962D-10460EC7C230}"

[Setup]
AppId={{B8E55779-4EDB-4D8F-962D-10460EC7C230}
AppName=Klicky
AppVersion={#AppVersion}
AppVerName=Klicky {#AppVersion} Windows development preview
AppPublisher=Muneer Alam and Sauhard Gupta
AppPublisherURL=https://github.com/Muneer320/klicky
AppSupportURL=https://github.com/Muneer320/klicky/issues
AppUpdatesURL=https://github.com/Muneer320/klicky/releases
AppComments=Unsigned development preview; physical and installation qualification remain open.
DefaultDirName={localappdata}\Programs\Klicky
DisableDirPage=yes
DisableProgramGroupPage=yes
UsePreviousAppDir=no
UsePreviousTasks=no
PrivilegesRequired=lowest
SetupArchitecture=x64
ArchitecturesAllowed=x64os
MinVersion=10.0.22000
SetupIconFile=klicky.ico
UninstallDisplayIcon={app}\klicky-desktop.exe
UninstallDisplayName=Klicky Windows development preview
UninstallLogMode=overwrite
CloseApplications=no
RestartApplications=no
RestartIfNeededByRun=no
AlwaysRestart=no
AllowCancelDuringInstall=no
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
OutputBaseFilename=klicky-{#AppVersion}-windows-x64-preview-{#Copy(SourceRevision, 1, 12)}-setup
VersionInfoVersion={#AppVersion}.0
VersionInfoProductTextVersion={#AppVersion} Windows development preview
VersionInfoDescription=Klicky unsigned development installer
VersionInfoOriginalFileName=klicky-setup.exe
SignedUninstaller=no

[Languages]
Name: english; MessagesFile: compiler:Default.isl

[Tasks]
Name: startup; Description: "Start Klicky when I sign in (Windows may independently disable this)"; Flags: unchecked; Check: FreshInstall

[Files]
Source: "{#PayloadDirectory}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\Klicky"; Filename: "{app}\klicky-desktop.exe"; Parameters: "--paused"; WorkingDir: "{app}"

[Run]
Filename: "{app}\klicky-desktop.exe"; Parameters: "--paused"; Description: "Open Klicky paused"; Flags: postinstall nowait skipifsilent unchecked; Check: OfferLaunch

[Code]
const
  MarkerName = '.klicky.bundle.lock';
  IdentityName = 'INSTALLER-IDENTITY.ini';
  GenericReadWrite = $C0000000;
  OpenAlways = 4;
  OpenReparsePoint = $00200000;
  ReparsePoint = $400;
  DirectoryAttribute = $10;
  InvalidAttributes = $FFFFFFFF;
var
  BundleHandle: THandle;
  Prepared, Committed, WasRunning, WasPaused, HadInstall, RemovalStarted, PostInstallWarning: Boolean;
  BackupDirectory: String;
  BackupFiles, BackupHashes: TStringList;
  BackupBytes: Int64;

function CreateFileW(Name: String; Access, Share: LongWord; Security: NativeUInt;
  Disposition, Flags: LongWord; Template: THandle): THandle;
  external 'CreateFileW@kernel32.dll stdcall';
function CloseHandle(Handle: THandle): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';
function GetFileAttributesW(Name: String): LongWord;
  external 'GetFileAttributesW@kernel32.dll stdcall';
function GetFileSize(Handle: THandle; var High: LongWord): LongWord;
  external 'GetFileSize@kernel32.dll stdcall';
function CreateDirectoryW(Name: String; Security: NativeUInt): Boolean;
  external 'CreateDirectoryW@kernel32.dll stdcall';

function FreshInstall: Boolean;
begin
  Result := not FileExists(ExpandConstant('{localappdata}\Programs\Klicky\') + IdentityName);
end;

procedure CheckPath(const Path: String; Directory: Boolean);
var
  Attributes: LongWord;
begin
  Attributes := GetFileAttributesW(Path);
  if (Attributes = InvalidAttributes) or ((Attributes and ReparsePoint) <> 0) or
     (((Attributes and DirectoryAttribute) <> 0) <> Directory) then
    RaiseException('Klicky requires an ordinary local path: ' + Path);
end;

procedure CheckParents(Path: String);
begin
  while Length(Path) > 3 do begin
    if GetFileAttributesW(Path) <> InvalidAttributes then CheckPath(Path, True);
    Path := ExtractFileDir(Path);
  end;
end;

procedure CheckInstallLocation;
var
  Path: String;
begin
  Path := ExpandConstant('{app}');
  if (Length(Path) < 3) or (Path[2] <> ':') or
     (CompareText(RemoveBackslashUnlessRoot(Path), ExpandConstant('{localappdata}\Programs\Klicky')) <> 0) then
    RaiseException('This preview installs only in the current user''s local Programs\Klicky folder. Custom or network install locations are not supported.');
  CheckParents(Path);
end;

procedure UnlockBundle;
begin
  if BundleHandle <> 0 then begin
    CloseHandle(BundleHandle);
    BundleHandle := 0;
  end;
end;

procedure LockBundle;
var
  Marker: String;
  High, Low: LongWord;
begin
  Marker := AddBackslash(ExpandConstant('{app}')) + MarkerName;
  BundleHandle := CreateFileW(Marker, GenericReadWrite, 0, 0, OpenAlways, OpenReparsePoint, 0);
  if BundleHandle = THandle(-1) then begin
    BundleHandle := 0;
    RaiseException('Another Klicky process or installer is using this bundle. Close Klicky in every signed-in session and retry. No process will be forced to close.');
  end;
  try
    CheckPath(Marker, False);
    High := 0;
    Low := GetFileSize(BundleHandle, High);
    if (Low <> 0) or (High <> 0) then RaiseException('The Klicky bundle ownership marker must be empty.');
  except
    UnlockBundle;
    RaiseException(GetExceptionMessage);
  end;
end;

function RunControl(const Arguments: String; RequireReceipt: Boolean): String;
var
  Code, I: Integer;
  Output: TExecOutput;
begin
  if not ExecAndCaptureOutput(ExpandConstant('{app}\klicky.exe'), Arguments,
       ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, Code, Output) then
    RaiseException('Could not run Klicky preparation: ' + SysErrorMessage(Code));
  if (Code <> 0) or Output.Error or (RequireReceipt and (GetArrayLength(Output.StdOut) <> 1)) then begin
    Log('Klicky hook exit=' + IntToStr(Code) + ', stdout lines=' + IntToStr(GetArrayLength(Output.StdOut)));
    for I := 0 to GetArrayLength(Output.StdOut) - 1 do
      if I < 16 then Log(Output.StdOut[I]);
    for I := 0 to GetArrayLength(Output.StdErr) - 1 do
      if I < 16 then Log(Output.StdErr[I]);
    RaiseException('Klicky could not confirm safe preparation. Close this bundle and retry. See the installer log for details.');
  end;
  Result := '';
  if RequireReceipt then Result := Trim(Output.StdOut[0]);
end;

procedure CheckIdentity;
var
  Identity, Expected, OldVersion: String;
  OldPacked, NewPacked: Int64;
begin
  Identity := ExpandConstant('{app}\') + IdentityName;
  CheckPath(Identity, False);
  if GetIniString('Klicky', 'AppId', '', Identity) <> '{#KlickyAppId}' then
    RaiseException('This folder is not owned by the Klicky installer. Move portable copies elsewhere first.');
  Expected := GetIniString('Klicky', 'CliSHA256', '', Identity);
  CheckPath(ExpandConstant('{app}\klicky.exe'), False);
  if (Length(Expected) <> 64) or
     (CompareText(GetSHA256OfFile(ExpandConstant('{app}\klicky.exe')), Expected) <> 0) then
    RaiseException('The installed control executable differs from its installer identity. Restore it before updating or removing Klicky.');
  if IsUninstaller then Exit;
  OldVersion := GetIniString('Klicky', 'Version', '', Identity);
  if not StrToVersion(OldVersion, OldPacked) or not StrToVersion('{#AppVersion}', NewPacked) then
    RaiseException('The installer version identity is invalid.');
  if ComparePackedVersion(OldPacked, NewPacked) > 0 then
    RaiseException('Downgrading Klicky is not supported. Remove the newer application first; your configuration and custom packs will remain.');
end;

procedure PrepareProcess;
var
  Receipt: String;
begin
  Receipt := RunControl('service prepare-update', True);
  if (Receipt <> 'KLICKY_UPDATE_V1 0 0') and (Receipt <> 'KLICKY_UPDATE_V1 0 1') and
     (Receipt <> 'KLICKY_UPDATE_V1 1 0') and (Receipt <> 'KLICKY_UPDATE_V1 1 1') then
    RaiseException('The installed Klicky version did not return a supported preparation receipt.');
  WasRunning := Receipt[18] = '1';
  WasPaused := Receipt[20] = '1';
end;

procedure SnapshotDirectory(const Relative: String; Depth: Integer);
var
  Found: TFindRec;
  Name, Source, Destination, Hash: String;
begin
  if Depth > 12 then RaiseException('Too many nested folders in the installed bundle.');
  Source := AddBackslash(ExpandConstant('{app}')) + Relative;
  CheckPath(RemoveBackslashUnlessRoot(Source), True);
  if FindFirst(AddBackslash(Source) + '*', Found) then try
    repeat
      if (Found.Name <> '.') and (Found.Name <> '..') then begin
        Name := AddBackslash(Relative) + Found.Name;
        if Relative = '' then Name := Found.Name;
        if (Found.Attributes and ReparsePoint) <> 0 then
          RaiseException('Reparse points are not permitted in the managed bundle: ' + Name);
        if (Found.Attributes and DirectoryAttribute) <> 0 then
          SnapshotDirectory(Name, Depth + 1)
        else if CompareText(Name, MarkerName) <> 0 then begin
          if (Found.SizeHigh <> 0) or (Found.SizeLow > 268435456) or (BackupFiles.Count >= 4096) then
            RaiseException('The managed bundle exceeds installer backup limits.');
          BackupBytes := BackupBytes + Found.SizeLow;
          if BackupBytes > 536870912 then RaiseException('The managed bundle exceeds the 512 MiB backup limit.');
          Source := AddBackslash(ExpandConstant('{app}')) + Name;
          Destination := AddBackslash(BackupDirectory) + 'payload\' + Name;
          if not ForceDirectories(ExtractFileDir(Destination)) then RaiseException('Cannot create recovery folder.');
          Hash := GetSHA256OfFile(Source);
          if not CopyFile(Source, Destination, True) then RaiseException('Cannot back up ' + Name);
          if CompareText(GetSHA256OfFile(Destination), Hash) <> 0 then RaiseException('Recovery copy failed verification: ' + Name);
          BackupFiles.Add(Name);
          BackupHashes.Add(Hash);
        end;
      end;
    until not FindNext(Found);
  finally
    FindClose(Found);
  end;
end;

procedure MakeBackup;
var
  Parent: String;
begin
  Parent := ExpandConstant('{localappdata}\KlickyInstaller\recovery');
  CheckParents(Parent);
  if not ForceDirectories(Parent) then RaiseException('Cannot create the recovery parent folder.');
  BackupDirectory := GenerateUniqueName(Parent, '.backup');
  if not CreateDirectoryW(BackupDirectory, 0) then begin
    BackupDirectory := '';
    RaiseException('Cannot create a new persistent recovery copy.');
  end;
  BackupFiles := TStringList.Create;
  BackupHashes := TStringList.Create;
  SnapshotDirectory('', 0);
  BackupFiles.SaveToFile(AddBackslash(BackupDirectory) + 'RECOVERY-FILES.txt');
  BackupHashes.SaveToFile(AddBackslash(BackupDirectory) + 'RECOVERY-SHA256.txt');
  Log('Verified previous bundle recovery copy: ' + BackupDirectory);
end;

function RestoreBackup: Boolean;
var
  I: Integer;
  Source, Destination: String;
begin
  Result := True;
  for I := 0 to BackupFiles.Count - 1 do begin
    Source := AddBackslash(BackupDirectory) + 'payload\' + BackupFiles[I];
    Destination := ExpandConstant('{app}\') + BackupFiles[I];
    try
      CheckParents(ExtractFileDir(Source));
      CheckPath(Source, False);
      CheckParents(ExtractFileDir(Destination));
      if FileExists(Destination) then CheckPath(Destination, False);
      if CompareText(GetSHA256OfFile(Source), BackupHashes[I]) <> 0 then
        Result := False
      else if not ForceDirectories(ExtractFileDir(Destination)) or
         not CopyFile(Source, Destination, False) or
         (CompareText(GetSHA256OfFile(Destination), BackupHashes[I]) <> 0) then Result := False;
    except
      Log(GetExceptionMessage);
      Result := False;
    end;
  end;
end;

procedure RestartPrevious;
var
  Arguments: String;
begin
  if not WasRunning then Exit;
  WasRunning := False;
  Arguments := 'service start';
  if WasPaused then Arguments := Arguments + ' --paused';
  try
    CheckIdentity;
    RunControl(Arguments, False);
  except
    PostInstallWarning := True;
    Log(GetExceptionMessage);
    SuppressibleMsgBox('Klicky files are ready, but restarting was not confirmed. Open Klicky from Start when ready.', mbError, MB_OK, IDOK);
  end;
end;

procedure CheckEmptyDestination;
var
  Found: TFindRec;
begin
  if not DirExists(ExpandConstant('{app}')) then Exit;
  if FindFirst(ExpandConstant('{app}\*'), Found) then try
    repeat
      if (Found.Name <> '.') and (Found.Name <> '..') and (Found.Name <> MarkerName) then
        RaiseException('The destination is not an empty installer-owned Klicky folder. Portable copies are preserved.');
    until not FindNext(Found);
  finally
    FindClose(Found);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if Prepared then Exit;
  try
    if BackupDirectory <> '' then RaiseException('A preparation backup already exists. Cancel this installer before retrying; the recovery copy is preserved at ' + BackupDirectory);
    CheckInstallLocation;
    HadInstall := FileExists(ExpandConstant('{app}\') + IdentityName);
    if HadInstall then begin
      CheckIdentity;
      PrepareProcess;
    end else CheckEmptyDestination;
    if not ForceDirectories(ExpandConstant('{app}')) then RaiseException('Cannot create the Klicky application folder.');
    LockBundle;
    if HadInstall <> FileExists(ExpandConstant('{app}\') + IdentityName) then
      RaiseException('Klicky installation ownership changed during preparation. Cancel and retry.');
    if HadInstall then begin
      CheckIdentity;
      MakeBackup;
    end else CheckEmptyDestination;
    Prepared := True;
  except
    Result := GetExceptionMessage;
    UnlockBundle;
    RestartPrevious;
  end;
end;

function OfferLaunch: Boolean;
begin
  Result := not HadInstall;
end;

procedure CurStepChanged(Step: TSetupStep);
begin
  if Step = ssPostInstall then begin
    Committed := True;
    UnlockBundle;
    if not HadInstall and WizardIsTaskSelected('startup') then try
      if RunControl('service register-startup', True) <> 'KLICKY_STARTUP_V1 REGISTERED' then
        RaiseException('Unexpected login registration response.');
    except
      PostInstallWarning := True;
      Log(GetExceptionMessage);
      SuppressibleMsgBox('Klicky was installed, but login startup could not be registered. Use its tray menu to inspect the conflict. Existing startup entries were preserved.', mbError, MB_OK, IDOK);
    end;
    RestartPrevious;
  end;
end;

procedure DeinitializeSetup;
var
  Restored: Boolean;
begin
  Restored := True;
  if Prepared and not Committed and HadInstall then Restored := RestoreBackup;
  UnlockBundle;
  if Prepared and not Committed and not HadInstall then begin
    DeleteFile(ExpandConstant('{app}\') + MarkerName);
    RemoveDir(ExpandConstant('{app}'));
  end;
  if Prepared and not Committed and Restored then RestartPrevious;
  if BackupDirectory <> '' then begin
    if Committed or (Prepared and Restored) then begin
      CheckParents(BackupDirectory);
      if not DelTree(BackupDirectory, True, True, True) then Log('Recovery copy cleanup incomplete: ' + BackupDirectory);
    end
    else
      SuppressibleMsgBox('The recovery copy was preserved at ' + BackupDirectory + '. Do not run a partial bundle; restore these files before retrying.', mbError, MB_OK, IDOK);
  end;
  if BackupFiles <> nil then BackupFiles.Free;
  if BackupHashes <> nil then BackupHashes.Free;
end;

function GetCustomSetupExitCode: Integer;
begin
  Result := 0;
  if PostInstallWarning then Result := 10;
end;

procedure CurUninstallStepChanged(Step: TUninstallStep);
var
  Receipt: String;
begin
  if Step = usAppMutexCheck then begin
    { Inno 7.1.1 calls this after confirmation and before any removal. Exceptions are fatal. }
    CheckInstallLocation;
    CheckIdentity;
    PrepareProcess;
    LockBundle;
    CheckIdentity;
  end else if Step = usUninstall then begin
    Receipt := RunControl('service unregister-startup', True);
    if (Receipt <> 'KLICKY_STARTUP_V1 ABSENT') and (Receipt <> 'KLICKY_STARTUP_V1 FOREIGN') then
      RaiseException('Login shortcut removal was not confirmed. No bundle files were removed.');
    RemovalStarted := True;
  end else if Step = usPostUninstall then begin
    UnlockBundle;
    DeleteFile(ExpandConstant('{app}\') + MarkerName);
    RemoveDir(ExpandConstant('{app}'));
  end;
end;

procedure DeinitializeUninstall;
begin
  UnlockBundle;
  if not RemovalStarted then RestartPrevious;
end;

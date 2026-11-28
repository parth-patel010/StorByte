# Bench tool

Times two things on a mounted StorByte drive:

- **Directory listing**: how long it takes to enumerate a folder.
- **First-byte latency**: how long it takes to open a large file, seek to an offset and read one byte. On a block that is not cached yet, this goes through the HTTP Range path.

It only reads from the mounted drive. It does not talk to the Go API, and it never reads or prints OAuth tokens. The report contains timings, the entry count and the file size, with no file names.

## Prerequisites

- Windows with Windows PowerShell 5.1 or later
- [WinFsp](https://winfsp.dev/) installed
- StorByte running with the drive mounted (`V:` by default)
- A large file on the drive that you have not opened recently. A few hundred MB or more gives the test enough separate 4 MB cache blocks.

## Run

```powershell
cd tools\bench
.\bench.ps1 -Drive V -SampleFile "V:\path\to\large-file.bin"
```

If script execution is disabled:

```powershell
powershell -ExecutionPolicy Bypass -File .\bench.ps1 -Drive V -SampleFile "V:\path\to\large-file.bin"
```

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `-Drive` | `V` | Mounted drive letter |
| `-ListPath` | drive root | Folder to list |
| `-SampleFile` | required | File for the first-byte test |
| `-Runs` | `5` | Listing passes and read offsets (2 to 100) |
| `-Json` | off | Print JSON instead of text |

## Reading the numbers

- `first` is only a cold number if nothing has touched the folder or file since the drive was mounted. For a true cold run, clear the cache, remount, and run the script before opening the drive in Explorer.
- `repeat` and `reread` show the cached path.
- Network speed dominates the cold numbers, so compare runs made on the same connection with the same file.

Sequential throughput and cache hit rate are not measured yet.

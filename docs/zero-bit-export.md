# Zero-bit export

When **Zero-bit export** is ON (default):

- Files on `V:\` are **streamed** on open (Drive HTTP range reads into a RAM block cache). Closing the file drops those blocks — Stroex does not keep a downloaded file body on the virtual drive.
- Explorer **Properties → Size on disk** may still look ≈ Size. That is a WinFsp/Windows limitation (allocation is clamped to file size for the cache manager), not a local download of the file.
- Copy or drag from `V:\` to a local folder writes a tiny `.stroexlink` JSON stub instead of downloading content.
- Opening a `.stroexlink` resolves to the virtual path (or streams via Stroex if mounted).

When OFF:

- Explorer copy/drag performs a normal read/write and downloads bytes to local disk.

Stub format:

```json
{
  "v": 1,
  "app": "stroex",
  "account_id": "...",
  "remote_id": "...",
  "name": "video.mp4",
  "size": 10737418240,
  "mime": "video/mp4",
  "virtual_path": "\\Videos\\video.mp4"
}
```

//! Pinned SHA-256 checksums of the official MuJoCo 3.12.0 release archives.
//!
//! Source: the archives attached to
//! <https://github.com/google-deepmind/mujoco/releases/tag/3.12.0>. The values
//! were computed from those downloads in milestone M0 (see
//! `docs/decisions/0002-mujoco-checksum-provenance.md`). Re-verify against the
//! release page before a release.

/// The MuJoCo version AXL is pinned to.
pub const MUJOCO_VERSION: &str = "3.12.0";

/// How an archive is unpacked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveKind {
    /// `.tar.gz` with one top-level folder.
    TarGz,
    /// `.zip` with the files at the top level.
    Zip,
    /// Apple disk image holding `mujoco.framework`.
    Dmg,
}

/// One downloadable MuJoCo archive.
#[derive(Clone, Copy, Debug)]
pub struct Archive {
    /// Folder name under `third_party/mujoco/<version>/`.
    pub os: &'static str,
    /// File name on the release page.
    pub file: &'static str,
    /// Lower-case hex SHA-256 of the file.
    pub sha256: &'static str,
    /// How to unpack it.
    pub kind: ArchiveKind,
}

/// Every supported archive.
pub const ARCHIVES: &[Archive] = &[
    Archive {
        os: "linux",
        file: "mujoco-3.12.0-linux-x86_64.tar.gz",
        sha256: "a9367911e6d5eaeade17c2197304687421c1fc932cdf7bcd4cb8cfaf0374dcb2",
        kind: ArchiveKind::TarGz,
    },
    Archive {
        os: "windows",
        file: "mujoco-3.12.0-windows-x86_64.zip",
        sha256: "ffe071c2747dd9513a1c59e7d2428bb678d887f9edec9eb9674b4288a248a8e9",
        kind: ArchiveKind::Zip,
    },
    Archive {
        os: "macos",
        file: "mujoco-3.12.0-macos-universal2.dmg",
        sha256: "8410882d724c3637b935dc0482b0de90efed44b17bbcfcb4a165ee46285a4865",
        kind: ArchiveKind::Dmg,
    },
];

/// The archive for an OS name (`linux`, `windows` or `macos`).
pub fn archive_for(os: &str) -> Option<&'static Archive> {
    ARCHIVES.iter().find(|a| a.os == os)
}

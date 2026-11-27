# Ressac for Fedora: the KDE app and the terminal app, packaged from binaries already built by
# `cargo build --release --locked -p ressac -p ressac-kde`. Built by build-rpm.sh, which defines:
#   ressac_version  the release version, "-" mapped to "~" (0.4.0-beta.1 -> 0.4.0~beta.1)
#   ressac_srcdir   the repository root (desktop entry, icon, licence texts)
#   ressac_bindir   cargo's release directory (the two binaries)
# The RPM never rebuilds from source, so it has no %%build and no debuginfo package.

%global debug_package %{nil}
%global _build_id_links none

Name:           ressac
Version:        %{ressac_version}
Release:        1%{?dist}
Summary:        Calm RSS reader for KDE Plasma and the terminal
# Workspace MIT; Bedstead CC0; Newsreader, Atkinson Hyperlegible Next, Cormorant Garamond,
# Cormorant SC and IBM Plex Mono OFL; d3 ISC. All are compiled into ressac-kde.
License:        MIT AND CC0-1.0 AND OFL-1.1 AND ISC
URL:            https://github.com/w3cdotorg/ressac
ExclusiveArch:  x86_64 aarch64

BuildRequires:  desktop-file-utils

# rpm derives the shared-library dependencies (Qt, KDE Frameworks' libraries, WebEngine) from the
# binaries. It cannot see what the app loads at run time through QML imports and the style, so:
# org.kde.kirigami (every page)
Requires:       kf6-kirigami%{?_isa}
# org.kde.desktop, the Qt Quick Controls style the app sets by default
Requires:       kf6-qqc2-desktop-style%{?_isa}
# QtQuick, QtQuick.Controls, .Layouts, .Shapes, .Dialogs and QtCore
Requires:       qt6-qtdeclarative%{?_isa}
# the QtWebEngine QML module (reader and Sea Chart) and its helper process
Requires:       qt6-qtwebengine%{?_isa}
# the QtWebChannel QML module (the Sea Chart's page bridge)
Requires:       qt6-qtwebchannel%{?_isa}
# owns /usr/share/icons/hicolor/scalable/apps
Requires:       hicolor-icon-theme
# The toolbar and menu icons (the app asks for the Breeze theme outside Plasma); without it the
# app works, with blank icons. Plasma always has it.
Recommends:     breeze-icon-theme

%description
Ressac tidies RSS subscriptions, then lets you read them calmly. This package holds the KDE
Plasma app (ressac-kde, with its launcher entry and icon) and the terminal app (ressac); both
share the database in ~/.local/share/ressac, which the package never touches.

%prep
cp -p %{ressac_srcdir}/apps/macos/Ressac/Resources/Fonts/*-OFL.txt \
      %{ressac_srcdir}/apps/macos/Ressac/Resources/Fonts/Bedstead-CC0.txt \
      %{ressac_srcdir}/apps/macos/Ressac/Resources/Chart/LICENSE-d3.txt \
      %{ressac_srcdir}/apps/macos/Ressac/Resources/Chart/fonts/*-OFL.txt \
      .

%install
install -Dm755 %{ressac_bindir}/ressac %{buildroot}%{_bindir}/ressac
install -Dm755 %{ressac_bindir}/ressac-kde %{buildroot}%{_bindir}/ressac-kde
install -Dm644 %{ressac_srcdir}/apps/linux/packaging/org.w3cdotorg.ressac.desktop \
    %{buildroot}%{_datadir}/applications/org.w3cdotorg.ressac.desktop
install -Dm644 %{ressac_srcdir}/apps/macos/Ressac/Resources/AppIcon.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/org.w3cdotorg.ressac.svg

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/org.w3cdotorg.ressac.desktop

%files
%license *-OFL.txt Bedstead-CC0.txt LICENSE-d3.txt
%{_bindir}/ressac
%{_bindir}/ressac-kde
%{_datadir}/applications/org.w3cdotorg.ressac.desktop
%{_datadir}/icons/hicolor/scalable/apps/org.w3cdotorg.ressac.svg

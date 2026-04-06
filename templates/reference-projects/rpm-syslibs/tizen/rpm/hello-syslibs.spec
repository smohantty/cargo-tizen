Name:           hello-syslibs
Version:        0.1.0
Release:        1%{?dist}
Summary:        Reference package linking against platform OpenSSL and SQLite
License:        Apache-2.0
BuildArch:      aarch64
Source0:        hello-syslibs

Requires:       openssl
Requires:       sqlite

%description
Regression reference for cargo-tizen cross-builds that link against system
libraries (libssl, libcrypto, libsqlite3) from the Tizen platform sysroot
instead of bundling them statically.

%prep

%build

%install
install -Dm0755 %{SOURCE0} %{buildroot}/usr/bin/hello-syslibs

%files
/usr/bin/hello-syslibs

Name:           hello-rpm
Version:        0.1.0
Release:        1%{?dist}
Summary:        Minimal cargo-tizen RPM reference package
License:        Apache-2.0
BuildArch:      aarch64
Source0:        hello-rpm

%description
Reference spec for a minimal cargo-tizen packaged binary.

%prep

%build

%install
install -Dm0755 %{SOURCE0} %{buildroot}/usr/bin/hello-rpm

%files
/usr/bin/hello-rpm


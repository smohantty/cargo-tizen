Name:           hello-multi
Version:        0.1.0
Release:        1%{?dist}
Summary:        Multi-package cargo-tizen RPM reference
License:        Apache-2.0
BuildArch:      aarch64
Source0:        hello-server
Source1:        hello-cli

%description
Reference spec for a multi-binary cargo-tizen RPM package.
Both hello-server and hello-cli are packaged into a single RPM.

%prep

%build

%install
install -Dm0755 %{SOURCE0} %{buildroot}/usr/bin/hello-server
install -Dm0755 %{SOURCE1} %{buildroot}/usr/bin/hello-cli

%files
/usr/bin/hello-server
/usr/bin/hello-cli

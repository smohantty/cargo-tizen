%global debug_package %{nil}
%global _build_id_links none

Name:           hello-service
Version:        0.1.0
Release:        1%{?dist}
Summary:        Reference cargo-tizen RPM with extra sources
License:        Apache-2.0
BuildArch:      aarch64
AutoReqProv:    no

Source0:        hello-service
Source1:        hello-service.service
Source2:        hello-service.env

%description
Reference spec demonstrating extra RPM source files with cargo-tizen.
Service file and environment config are provided as Source1/Source2 in
tizen/rpm/sources/ instead of being generated inline in the spec.

%prep

%build

%install
install -Dm0755 %{SOURCE0} %{buildroot}/usr/bin/hello-service
install -Dm0644 %{SOURCE1} %{buildroot}/usr/lib/systemd/system/hello-service.service
install -Dm0644 %{SOURCE2} %{buildroot}/etc/hello-service.env

%pre
if [ "$1" -gt 1 ] && command -v systemctl >/dev/null 2>&1; then
    systemctl stop hello-service.service >/dev/null 2>&1 || true
fi

%post
if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload >/dev/null 2>&1 || true
    systemctl enable hello-service.service >/dev/null 2>&1 || true
    systemctl restart hello-service.service >/dev/null 2>&1 || true
fi

%preun
if [ "$1" -eq 0 ] && command -v systemctl >/dev/null 2>&1; then
    systemctl stop hello-service.service >/dev/null 2>&1 || true
    systemctl disable hello-service.service >/dev/null 2>&1 || true
fi

%postun
if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload >/dev/null 2>&1 || true
fi

%files
%attr(0755,root,root) /usr/bin/hello-service
%attr(0644,root,root) /usr/lib/systemd/system/hello-service.service
%config(noreplace) %attr(0644,root,root) /etc/hello-service.env

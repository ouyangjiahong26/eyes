%define __spec_install_post %{nil}
%define __os_install_post %{_dbpath}/brp-compress
%define debug_package %{nil}

Name: eyes
Summary: 桌面坐姿监测与护眼提醒工具
Version: @@VERSION@@
Release: @@RELEASE@@%{?dist}
License: MIT
Group: Applications/System
Source0: %{name}-%{version}.tar.gz
BuildRoot: %{_tmppath}/%{name}-%{version}-%{release}-root

%description
通过摄像头监测头部姿态，提醒你保持正确坐姿、适时休息。

%prep
%setup -q

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
cp -a * %{buildroot}

%clean
rm -rf %{buildroot}

%files
%defattr(-,root,root,-)
%{_bindir}/eyes
/usr/lib/eyes/models/face_detection_yunet_2023mar.onnx
/usr/lib/eyes/models/MANIFEST.toml
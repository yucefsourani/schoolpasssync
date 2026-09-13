app_id = 'com.github.yucefsourani.schoolpasssync'

# إعداد أمر Cargo بحيث يفرض واجهة رسومية (بدون DOS) على ويندوز
if host_machine.system() == 'windows'
  cargo_cmd = 'RUSTFLAGS="-C link-arg=-mwindows" cargo build --manifest-path "$1" $2 && cp "$3/target/$4/$5" "$6"'
else
  cargo_cmd = 'cargo build --manifest-path "$1" $2 && cp "$3/target/$4/$5" "$6"'
endif

# 1. بناء البرنامج باستخدام Cargo ونسخه إلى مسار المخرجات
custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: exe_name,
  console: true,
  install: true,
  install_dir: bindir,
  command: [
    'sh', '-c',
    cargo_cmd,
    '_', # يمثل $0
    meson.project_source_root() / 'Cargo.toml', # $1
    get_option('buildtype') == 'release' ? '--release' : '', # $2
    meson.project_source_root(), # $3
    rust_target, # $4
    exe_name, # $5
    '@OUTPUT@' # $6
  ]
)

# 2. تثبيت ملفات الاختصار والأيقونة لنظام لينكس/يونكس فقط
if host_machine.system() != 'windows'
  # تثبيت الأيقونة في المسار القياسي للينكس
  install_data(
    app_id + '.png',
    install_dir: datadir / 'icons' / 'hicolor' / '256x256' / 'apps'
  )

  # تثبيت ملف الـ Desktop
  install_data(
    app_id + '.desktop',
    install_dir: datadir / 'applications'
  )
endif

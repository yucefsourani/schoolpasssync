project('schoolpasssync', 'rust',
  version: '0.1.0',
  meson_version: '>= 0.59.0'
)

# مسارات التثبيت القياسية
prefix = get_option('prefix')
bindir = prefix / get_option('bindir')
datadir = prefix / get_option('datadir')

# التحقق من توفر مكتبات الواجهة الرسومية
dependency('gtk4', version: '>= 4.10')
dependency('libadwaita-1', version: '>= 1.2')

cargo = find_program('cargo', required: true)

# إعداد خيارات البناء بناءً على نوع الـ Build (Release أو Debug)
if get_option('buildtype') == 'release'
  rust_target = 'release'
else
  rust_target = 'debug'
endif

# تحديد امتداد الملف التنفيذي ليتوافق مع ويندوز
if host_machine.system() == 'windows'
  exe_name = meson.project_name() + '.exe'
else
  exe_name = meson.project_name()
endif

# الانتقال إلى مجلد src
subdir('src')

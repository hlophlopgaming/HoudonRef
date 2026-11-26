# HoudonRef user interface, Russian.

app-title = HoudonRef
section-atlas = Атлас
section-studio = Студия
section-atelier = Ателье
language = Язык

atlas-coming = Анатомический атлас в работе: экорше по слоям, от кожи до скелета, с названиями на английском, русском и латыни.
atelier-coming = Ателье для рисования в работе: сессии на время, от шара Лумиса через плоскости к детальной голове, со сменой ракурса и света.

model-stats = Треугольников: { $triangles }

studio-objects = Объекты
studio-object-duplicate = Дублировать
studio-object-delete = Удалить
studio-frame-scene = Показать всю сцену
studio-frame-selected = Показать выделенное
studio-selected-object = Выделенный объект
studio-no-selection = Щёлкните объект в окне или в списке, чтобы выделить его.
studio-position = Положение
studio-rotation = Поворот
studio-size = Размер
studio-camera-hint =
    Тянуть мышью: вращать вокруг сцены
    Правой кнопкой или Shift + тянуть: сдвигать
    Колесо или щипок: ближе или дальше
    Щелчок: выделить · двойной щелчок: показать целиком
    Delete: удалить выделенный объект
    V: включить и выключить тональный разбор
    P: презентация, только вид (P или Esc — выйти)
studio-drop-hint = Перетащите файл .glb или .gltf в окно, чтобы добавить его.

studio-camera = Камера
studio-focal-length = Фокусное расстояние

studio-light-azimuth = Направление
studio-light-elevation = Высота
studio-light-intensity = Яркость

error-blend-file = Файлы .blend пока нельзя открыть напрямую. В Blender выберите File → Export → glTF 2.0 (.glb) и откройте получившийся файл.
error-unsupported-file = Неподдерживаемый тип файла: { $name }. Перетащите модель (.glb или .gltf), изображение или сцену (.ron).
error-load-failed = Не удалось открыть { $name }: { $reason }
error-read-failed = Не удалось прочитать { $name }.
scene-open = Открыть сцену
scene-save = Сохранить сцену
scene-embed-models = Встроить модели в файл сцены
scene-file-type = Сцена HoudonRef
scene-open-error = Не удалось открыть сцену { $name }: { $reason }
scene-save-error = Не удалось сохранить сцену: { $reason }
scene-model-skipped = Не удалось загрузить модель { $name }. Её объекты пропущены.

studio-object-label = { $model } { $number }
unit-metre-short = м
unit-millimetre-short = мм
model-male-figure = мужская фигура
model-female-figure = женская фигура

settings = Настройки
settings-units = Единицы измерения
units-metric = Метрические (см, м)
units-imperial = Имперские (дюймы, футы)
settings-figure-height = Рост фигуры
settings-anti-aliasing = Сглаживание краёв
settings-anti-aliasing-hint = Выключите, если вьюпорт тормозит на слабом компьютере или телефоне.
settings-quality = Качество отрисовки
settings-quality-auto = Авто (сейчас: { $level })
settings-quality-low = Низкое
settings-quality-medium = Среднее
settings-quality-high = Высокое
settings-quality-hint = Чёткие тени и гладкая окклюзия стоят скорости. Авто выбирает по видеокарте и понижает, если вьюпорт долго тормозит.
studio-figure = Фигура человека (рост из настроек)
studio-dimensions = Ширина { $width }, высота { $height }, глубина { $depth }
studio-real-size = Реальный размер (по большей стороне)
studio-real-size-warning = Размер в этом файле выглядит неправдоподобно (меньше { $smallest } или больше { $largest }). Укажите реальный размер.
unit-centimetre-short = см
unit-inch-short = дюйм
unit-foot-short = фут
length = { $value } { $unit }

studio-lights = Свет
studio-lights-full = В сцене может быть не больше { $count } источников.
studio-light-label = { $number }. { $kind }
studio-light-remove = Удалить
studio-light-distance = Расстояние
studio-light-cone = Конус
studio-light-softness = Мягкость края
light-directional = Направленный
light-point = Точечный
light-spot = Прожектор

studio-room = Окружение
studio-room-visible = Показывать пол и задник
studio-room-floor = Пол
studio-room-backdrop = Задник
room-grey = Серая студия
room-white = Белая студия
room-dark = Тёмная студия
studio-ambient = Окружающий свет
studio-camera-direction = Направление
studio-camera-height = Высота
studio-camera-distance = Расстояние
studio-light-casts-shadows = Отбрасывает тени
studio-light-shadow-softness = Мягкость тени
studio-light-shadow-softness-hint = Насколько большим выглядит источник: 0 — голая лампа или солнце с чёткими тенями, 1 — большой софтбокс. Чем дальше тень от предмета, тем сильнее она размывается.
studio-light-mask = Маска
studio-light-mask-none = Нет
studio-light-mask-remove = Убрать маску
studio-light-mask-pattern = Размер узора
mask-picker = Маски света
mask-picker-hint = Щёлкните маску, чтобы надеть её на выбранный источник. Перетащите в окно PNG или JPG, чтобы взять свою.
mask-credit = Маски света: Kenney (kenney.nl), CC0.
mask-unavailable = Недоступна
mask-category-windows = Окна
mask-category-foliage = Листва
mask-category-caustics = Вода
mask-category-cones = Конусы
mask-category-streaks = Лучи
mask-category-circles = Круги
mask-category-fans = Вентиляторы
mask-category-shapes = Пятна
error-mask-needs-light = Чтобы сделать картинку маской света, сначала выберите прожектор или направленный свет, потом перетащите картинку.
error-mask-unreadable = { $name } — не PNG или JPEG, или файл не читается.

studio-shading = Затенение
shading-solid = Solid
shading-lit = Lit
shading-render = Render
shading-render-later = Появится в следующих версиях: качественный рендер на компьютере.
studio-cavity = Рёбра и впадины (cavity)
cavity-hint = Высветляет рёбра и затемняет впадины, чтобы читались мелкие изменения формы. Работает в Solid и Lit.
cavity-geometry = По геометрии
cavity-screen = По экрану
cavity-ridge = Рёбра
cavity-hollow = Впадины
studio-contours = Контурные линии
contours-hint = Силуэт и внутренние изломы формы линиями.
contours-thickness = Толщина
contours-folds = Изломы
studio-wireframe = Сетка
wireframe-opacity = Непрозрачность
studio-backface-culling = Скрывать изнанку
backface-culling-hint = Рисовать поверхности только с лицевой стороны. Импортированные модели часто открытые, и тогда их внутренняя сторона пропадёт.
solid-studio = Студия
solid-matcap = Мэткап
solid-flat = Плоский
studio-matcap = Мэткап
studio-matcap-none = Нет (выберите)
matcap-picker = Мэткапы
matcap-picker-hint = Щёлкните мэткап, чтобы выбрать его. Пока это окно открыто, перетащите в окно EXR, PNG или JPG, чтобы взять свой.
matcap-credit = Мэткапы сообщества Blender (CC0 или общественное достояние).
error-matcap-unreadable = { $name } — не EXR, PNG или JPEG, или файл не читается.

studio-color-source = Цвет
color-material = Материалы
color-clay = Глина
color-random = Случайный
studio-clay-tone = Тон глины

studio-values = Тоны
values-off = Выкл
values-grey = Серый
values-tones = { $count ->
    [one] { $count } тон
    [few] { $count } тона
   *[many] { $count } тонов
}
values-balance = Баланс света и тени
values-squint = Прищур

model-asaro-head = голова Асаро
model-planar-head = планарная голова
model-base-head = базовая голова
model-wolfsaro-head = голова волка

light-presets = Пресеты света
light-preset-choose = Выберите пресет
light-preset-apply = Применить
light-preset-import = Импорт…
light-preset-export = Экспорт…
light-preset-name = Название пресета
light-preset-save-current = Сохранить текущий свет
light-preset-file-type = Пресет света HoudonRef
light-preset-builtin-three-point = Трёхточечный свет
light-preset-builtin-rembrandt = Свет Рембрандта
light-preset-builtin-rim = Контровой свет
light-preset-builtin-top = Верхний свет
light-preset-builtin-sunrise = Рассвет
light-preset-builtin-cloudy = Пасмурное небо
light-preset-applied = Пресет света применён.
light-preset-saved = Пресет света сохранён.
light-preset-imported = Пресет света импортирован.
light-preset-duplicate = Пресет с таким названием уже сохранён.
light-preset-load-error = Не удалось прочитать сохранённые пресеты света: { $reason }
light-preset-save-error = Не удалось сохранить пресет света: { $reason }
light-preset-import-error = Не удалось импортировать пресет света: { $reason }
light-preset-export-error = Не удалось экспортировать пресет света: { $reason }

toolbar-scene = Сцена
toolbar-add = Добавить
toolbar-open-model = Открыть файл модели…
toolbar-values = Тоны
toolbar-frame-scene-hint = Показать всю сцену (двойной щелчок по пустому месту)
toolbar-frame-selected-hint = Показать выделенный объект (двойной щелчок по нему)
toolbar-help = Мышь и клавиши
menu-figures = Фигуры
menu-heads = Головы
menu-male-figure = Мужская фигура (тест)
menu-female-figure = Женская фигура (тест)
menu-asaro-head = Голова Асаро
menu-planar-head = Планарная голова
menu-base-head = Базовая голова
menu-wolfsaro-head = Голова волка (Wolfsaro)
model-file-type = 3D-модели
image-file-type = Изображения
studio-display = Отображение
studio-no-objects = Объектов пока нет: добавьте их через «Добавить» на панели над окном.
studio-empty-viewport = Добавьте модель: меню «Добавить» сверху или перетащите сюда файл .glb или .gltf.
studio-add-light-menu = + Добавить свет
shading-solid-hint = Постоянный свет, чтобы читать форму; свет сцены не учитывается.
shading-lit-hint = Свет сцены, тени и окружение.
values-key-hint = Клавиша V над окном включает и выключает тональный разбор.
mask-open-file = Открыть свою картинку…
matcap-open-file = Открыть свой мэткап…

camera-preset-front = Спереди
camera-preset-three-quarter = Три четверти
camera-preset-profile = Профиль
camera-preset-from-above = Сверху
camera-preset-from-below = Снизу
camera-preset-lens = Объектив { $mm } мм
camera-target = Кадрировать:
camera-target-scene = всю сцену
camera-target-selected = выделенный объект
camera-presets-mine = Мои пресеты камеры
room-presets-mine = Мои окружения
preset-none-saved = Пока ничего не сохранено.
preset-choose = Выберите пресет
preset-apply = Применить
preset-export = Экспорт…
preset-import = Импортировать файл пресета…
preset-name = Название
preset-save-current = Сохранить текущее
preset-file-type = Пресет HoudonRef
preset-saved = Пресет сохранён.
preset-imported = Пресет импортирован.
preset-duplicate = Пресет с таким названием уже сохранён.
preset-load-error = Не удалось прочитать сохранённые пресеты: { $reason }
preset-save-error = Не удалось сохранить пресет: { $reason }
preset-import-error = Не удалось импортировать пресет: { $reason }
preset-export-error = Не удалось экспортировать пресет: { $reason }

studio-ambient-occlusion = Затенение в углах (AO)
ambient-occlusion-hint = Затемняет углы, места касания, складки и впадины, куда меньше попадает свет окружения. Только в режиме Lit.
ambient-occlusion-strength = Сила
ambient-occlusion-radius = Радиус
ambient-occlusion-samples = Качество
ambient-occlusion-samples-hint = Выборок на пиксель: больше — глаже, но медленнее.

export-title = Сохранить вид картинкой
export-window = PNG, размер окна…
export-double = PNG, вдвое больше окна…
export-large = PNG, 4096 пикселей…
png-file-type = Картинка PNG
export-error = Не удалось сохранить картинку: { $reason }
toolbar-presentation = Презентация
toolbar-presentation-hint = Скрыть все панели и показать только вид (P). P или Esc возвращают их.

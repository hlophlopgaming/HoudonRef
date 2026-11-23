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
studio-add-male = Добавить мужскую фигуру (тест)
studio-add-female = Добавить женскую фигуру (тест)
studio-object-duplicate = Дублировать
studio-object-delete = Удалить
studio-frame-scene = Показать всю сцену
studio-frame-selected = Показать выделенное
studio-selected-object = Выделенный объект
studio-no-selection = Щёлкните объект в окне или в списке, чтобы выделить его.
studio-position = Положение
studio-rotation = Поворот
studio-size = Размер
studio-camera-hint = Тяните мышью, чтобы вращать; правой кнопкой или с Shift, чтобы сдвигать; колесом, чтобы приближать. Щелчок выделяет, двойной щелчок показывает целиком, Delete удаляет.
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
studio-figure = Фигура человека (рост из настроек)
studio-dimensions = Ширина { $width }, высота { $height }, глубина { $depth }
studio-real-size = Реальный размер (по большей стороне)
studio-real-size-warning = Размер в этом файле выглядит неправдоподобно (меньше { $smallest } или больше { $largest }). Укажите реальный размер.
unit-centimetre-short = см
unit-inch-short = дюйм
unit-foot-short = фут
length = { $value } { $unit }

studio-lights = Свет
studio-add-light = + { $kind }
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
values-hint = V во вьюпорте включает и выключает тональный разбор.

studio-add-asaro-head = Добавить голову Асаро
studio-add-planar-head = Добавить планарную голову
model-asaro-head = голова Асаро
model-planar-head = планарная голова
studio-add-base-head = Добавить базовую голову
model-base-head = базовая голова
studio-add-wolfsaro-head = Добавить голову волка (Wolfsaro)
model-wolfsaro-head = голова волка

//! Выбор товара из того, что нашлось в магазине.
//!
//! Магазин отдаёт дюжину вариантов на один запрос, и выбрать надо один: вслух
//! перечислять двенадцать позиций бессмысленно. Вопрос в том, какой.
//!
//! Первый попавшийся — это порядок выдачи магазина, то есть его собственные
//! соображения, а не выгода человека. Самый дешёвый — ловушка: на живой выдаче
//! по запросу «молоко» дешевле всего оказывается бутылка 450 мл за 89 ₽, тогда
//! как литр стоит 93 ₽. Заплатив на четыре рубля меньше, человек получает вдвое
//! меньше молока.
//!
//! Поэтому сравниваются не цены, а цена за литр или килограмм. Объём берётся из
//! названия — магазин пишет его там всегда: «Молоко 3,2%, 1 л», «Фарш из
//! индейки, 400 г». Там, где объём вытащить не удалось, товар не выбрасывается,
//! а уходит в конец очереди: лучше предложить непонятную упаковку, чем не
//! предложить ничего.

/// Товар с ценой, каким его прислал магазин.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub name: String,
    /// Рубли. `None` — цену со страницы вытащить не удалось.
    pub price: Option<u32>,
    pub available: bool,
    /// Адрес карточки — по нему товар потом кладётся в корзину.
    pub url: String,
}

/// Сколько товара в упаковке, приведённое к общей мере.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Amount {
    /// Литры — для всего жидкого.
    Litres(f64),
    /// Килограммы — для всего весового.
    Kilos(f64),
    /// Штуки — яйца, булочки и прочее счётное.
    Pieces(f64),
}

impl Amount {
    fn value(self) -> f64 {
        match self {
            Amount::Litres(value) | Amount::Kilos(value) | Amount::Pieces(value) => value,
        }
    }

    /// Сравнимы ли две меры. Литры с килограммами не сравниваются: «молоко
    /// 1 л» и «сыр 200 г» — разные вещи, и делить одно на другое нельзя.
    fn same_kind(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }
}

/// Выбирает лучший товар: дешевле всего за единицу меры.
///
/// Возвращает `None`, когда выбирать не из чего — все варианты недоступны или
/// список пуст.
pub fn best(candidates: &[Candidate]) -> Option<&Candidate> {
    let available: Vec<&Candidate> = candidates.iter().filter(|item| item.available).collect();
    let (first, _) = available.split_first()?;

    // Мера берётся у первого товара с распознанным объёмом. Выдача по одному
    // запросу однородна — молоко к молоку, — поэтому первая же распознанная
    // мера и задаёт, в чём считать остальные.
    let kind = available.iter().find_map(|item| amount_of(&item.name));

    let Some(kind) = kind else {
        // Объём не распознан ни у кого: сравнивать не по чему, остаётся цена.
        return available
            .iter()
            .filter(|item| item.price.is_some())
            .min_by_key(|item| item.price.unwrap_or(u32::MAX))
            .or(Some(first))
            .copied();
    };

    let scored: Vec<(&Candidate, f64)> = available
        .iter()
        .filter_map(|item| {
            let price = item.price? as f64;
            let amount = amount_of(&item.name)?;
            // Разнородное в счёт не идёт: сравнивать литры с килограммами нельзя.
            (amount.same_kind(kind) && amount.value() > 0.0)
                .then(|| (*item, price / amount.value()))
        })
        .collect();

    scored
        .into_iter()
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(item, _)| item)
        // Ни у кого не вышло посчитать цену за меру — отдаём первый доступный.
        .or(Some(first))
}

/// Достаёт объём из названия товара.
///
/// Разбирается последнее число с мерой: в названиях сначала идёт содержание
/// («Молоко 3,2%, 1 л»), и первое число — это жирность, а не объём.
fn amount_of(name: &str) -> Option<Amount> {
    let lower = name.to_lowercase().replace(',', ".");
    let bytes: Vec<char> = lower.chars().collect();

    let mut found = None;
    let mut at = 0;
    while at < bytes.len() {
        if !bytes[at].is_ascii_digit() {
            at += 1;
            continue;
        }

        let start = at;
        while at < bytes.len() && (bytes[at].is_ascii_digit() || bytes[at] == '.') {
            at += 1;
        }
        let number: String = bytes[start..at].iter().collect();
        let Ok(value) = number.trim_end_matches('.').parse::<f64>() else {
            continue;
        };

        // Пробел между числом и мерой необязателен: «900мл» и «900 мл».
        let tail: String = bytes[at..].iter().collect();
        let tail = tail.trim_start();
        if let Some(amount) = unit_of(tail, value) {
            found = Some(amount);
        }
    }
    found
}

/// Мера, с которой начинается хвост названия.
///
/// Порядок проверки важен: «мл» должно проверяться раньше «л», иначе
/// девятьсот миллилитров прочитаются как девятьсот литров.
fn unit_of(tail: &str, value: f64) -> Option<Amount> {
    const MILLILITRES: &[&str] = &["мл", "ml"];
    const LITRES: &[&str] = &["л", "l"];
    const GRAMS: &[&str] = &["г", "гр", "g"];
    const KILOS: &[&str] = &["кг", "kg"];
    const PIECES: &[&str] = &["шт", "штук"];

    // Килограммы раньше граммов по той же причине, что миллилитры раньше литров.
    for unit in KILOS {
        if starts_with_unit(tail, unit) {
            return Some(Amount::Kilos(value));
        }
    }
    for unit in MILLILITRES {
        if starts_with_unit(tail, unit) {
            return Some(Amount::Litres(value / 1000.0));
        }
    }
    for unit in GRAMS {
        if starts_with_unit(tail, unit) {
            return Some(Amount::Kilos(value / 1000.0));
        }
    }
    for unit in LITRES {
        if starts_with_unit(tail, unit) {
            return Some(Amount::Litres(value));
        }
    }
    for unit in PIECES {
        if starts_with_unit(tail, unit) {
            return Some(Amount::Pieces(value));
        }
    }
    None
}

/// Начинается ли хвост с этой меры — и кончается ли мера там же.
///
/// Без проверки конца «л» нашлась бы в слове «лапша», а «г» — в «городской».
fn starts_with_unit(tail: &str, unit: &str) -> bool {
    let Some(rest) = tail.strip_prefix(unit) else {
        return false;
    };
    match rest.chars().next() {
        None => true,
        Some(next) => !next.is_alphanumeric(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, price: u32) -> Candidate {
        Candidate {
            name: name.into(),
            price: Some(price),
            available: true,
            url: String::new(),
        }
    }

    #[test]
    fn volume_is_read_from_the_name() {
        assert_eq!(amount_of("Молоко 3,2%, 1 л"), Some(Amount::Litres(1.0)));
        assert_eq!(
            amount_of("Молоко 2,5% в бутылке, 900 мл"),
            Some(Amount::Litres(0.9))
        );
        assert_eq!(
            amount_of("Фарш из индейки, 400 г"),
            Some(Amount::Kilos(0.4))
        );
        assert_eq!(amount_of("Сахар, 1 кг"), Some(Amount::Kilos(1.0)));
        assert_eq!(amount_of("Яйцо куриное, 10 шт"), Some(Amount::Pieces(10.0)));

        // Первое число — жирность, а не объём: берётся последнее.
        assert_eq!(
            amount_of("Молоко Чабан отборное 3,4-4,5% канистра 1,85 л"),
            Some(Amount::Litres(1.85))
        );

        // Мера, склеенная с числом, и мера внутри слова.
        assert_eq!(amount_of("Кефир 900мл"), Some(Amount::Litres(0.9)));
        assert_eq!(amount_of("Лапша быстрого приготовления"), None);
    }

    #[test]
    fn the_cheapest_per_litre_wins_not_the_cheapest() {
        // Настоящая выдача ВкусВилла по запросу «молоко».
        let shelf = [
            item("Молоко 2,5% в бутылке, 900 мл", 100),
            item("Молоко 3,2%, 1 л", 93),
            item("Молоко цельное в бутылке, 900 мл", 107),
            item("Молоко 3,2% в бутылке, 450 мл", 89),
        ];

        let picked = best(&shelf).expect("выбор сделан");
        assert_eq!(
            picked.name, "Молоко 3,2%, 1 л",
            "литр за 93 выгоднее, чем 450 мл за 89"
        );
    }

    /// Вся живая выдача ВкусВилла по запросу «молоко», как она есть.
    ///
    /// Выборка из четырёх позиций доказывает правило, а этот список — что оно
    /// выдерживает настоящий магазин. Здесь и молоко в граммах, и молоко вовсе
    /// без меры, и полуторалитровые канистры: всё то, обо что выбор спотыкается
    /// на практике, а не в примере.
    #[test]
    fn the_real_shelf_is_handled() {
        let shelf = [
            item("Молоко 2,5% в бутылке, 900 мл", 100),
            item("Молоко 3,2%, 1 л", 93),
            item("Молоко цельное в бутылке, 900 мл", 107),
            item("Молоко 3,2% в бутылке, 900 мл", 104),
            item("Молоко Село Зеленое питьевое ультрапастеризованное 3,2% 950 мл", 154),
            item("Молоко Чабан отборное 3,4-4,5% канистра 1,85 л", 299),
            item("Молоко ЭкоНива ультрапастеризованное 2,5% 1 л", 149),
            item("Молоко козье цельное, 450 мл", 193),
            // Молоко, померенное в граммах: в литрах его не сравнить.
            item("Молоко 3,2% в бутылке, 1400 г", 164),
            // Молоко вовсе без меры в названии.
            item("Молоко козье 2,8-4%", 180),
            item("Молоко безлактозное ультрапастеризованное 1,5%, 970 мл", 156),
            item("Молоко Правильное молоко пастеризованное 3,2-4% 2 л", 310),
            item("Молоко Parmalat Comfort Безлактозное 1,8% 1 л", 199),
            item("Молоко Северная Долина ультрапастеризованное безлактозное 1,5% 950 г", 130),
            item("Молоко Parmalat ультрапастеризованное 3,5% 1 л", 179),
            item("Молоко безлактозное ультрапастеризованное 3,2%, 970 мл", 172),
            item("Молоко безлактозное 2,5%, 900 мл", 142),
            item("Молоко Можайское топленое стерилизованное 3,2% 450 мл", 129),
            item("Молоко 3,2% в бутылке, 450 мл", 89),
            item("Молоко Parmalat ультрапастеризованное 1,8% 1 л", 169),
        ];

        let picked = best(&shelf).expect("выбор сделан");
        assert_eq!(
            picked.name, "Молоко 3,2%, 1 л",
            "литр за 93 рубля — 93 рубля за литр, дешевле всех на полке"
        );

        // Проверка от противного: самое дешёвое и лучшее — разные товары,
        // иначе тест не доказывал бы ничего.
        let cheapest = shelf
            .iter()
            .filter(|item| item.available)
            .min_by_key(|item| item.price.unwrap_or(u32::MAX))
            .expect("полка не пуста");
        assert_eq!(cheapest.name, "Молоко 3,2% в бутылке, 450 мл");
        assert_ne!(
            cheapest.name, picked.name,
            "выбор по цене за литр обязан отличаться от выбора по цене"
        );
    }

    #[test]
    fn sold_out_goods_are_not_offered() {
        let shelf = [
            Candidate {
                name: "Молоко 3,2%, 1 л".into(),
                price: Some(50),
                available: false,
                url: String::new(),
            },
            item("Молоко 2,5%, 1 л", 93),
        ];
        assert_eq!(best(&shelf).map(|item| item.name.as_str()), Some("Молоко 2,5%, 1 л"));

        let empty: Vec<Candidate> = Vec::new();
        assert!(best(&empty).is_none(), "выбирать не из чего");
    }

    #[test]
    fn goods_without_a_volume_still_get_offered() {
        // Ни у кого нет меры — сравнивать не по чему, остаётся цена.
        let shelf = [item("Хлеб бородинский", 80), item("Хлеб дарницкий", 65)];
        assert_eq!(
            best(&shelf).map(|item| item.name.as_str()),
            Some("Хлеб дарницкий")
        );
    }

    #[test]
    fn litres_are_not_compared_with_kilos() {
        // Мера задаётся первым распознанным товаром — литрами. Килограммовый
        // сыр в сравнение не идёт, иначе он выиграл бы по «цене за единицу».
        let shelf = [
            item("Молоко 3,2%, 1 л", 93),
            item("Сыр российский, 200 г", 190),
            item("Молоко 2,5%, 900 мл", 100),
        ];
        assert_eq!(
            best(&shelf).map(|item| item.name.as_str()),
            Some("Молоко 3,2%, 1 л")
        );
    }
}

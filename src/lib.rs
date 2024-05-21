use chrono::{Datelike, NaiveTime, Weekday};
use serde::de::{IntoDeserializer, Visitor};

use serde::{Deserialize, Deserializer};
use std::fmt::Display;
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::Mutex;

fn space_separated<'de, V, D>(deserializer: D) -> Result<V, D::Error>
where
    V: FromIterator<Weekday>,
    D: Deserializer<'de>,
{
    struct SpaceSeparated<V, T>(PhantomData<V>, PhantomData<T>);

    impl<'de, V, T> Visitor<'de> for SpaceSeparated<V, T>
    where
        V: FromIterator<Weekday>,
        T: FromStr,
        T::Err: Display,
    {
        type Value = V;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("space separated days")
        }
        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let iter = v
                .split_whitespace()
                .map(|v| v.to_string())
                .map(|v| Weekday::deserialize(v.into_deserializer()));
            Result::from_iter(iter)
        }
    }
    let visitor = SpaceSeparated(PhantomData, PhantomData::<String>);
    deserializer.deserialize_str(visitor)
}
//This is deserialized from a CSV file then converted to ClassTime
#[derive(Debug, Deserialize)]
pub struct CSVTime {
    #[serde(deserialize_with = "space_separated")]
    days: Vec<Weekday>,
    name: String,
    begin: String,
    end: String,
}
//exists to call NaiveTime::parse_from_str bc serde can't
#[derive(Debug)]
pub struct ClassTime {
    class: String,
    days: Vec<Weekday>,
    begin: NaiveTime,
    end: NaiveTime,
}
impl From<CSVTime> for ClassTime {
    fn from(value: CSVTime) -> Self {
        ClassTime {
            class: value.name,
            days: value.days,
            begin: NaiveTime::parse_from_str(value.begin.as_str(), "%H:%M").unwrap(),
            end: NaiveTime::parse_from_str(value.end.as_str(), "%H:%M").unwrap(),
        }
    }
}
//We use this bc we can't format Chrono::Duration with a custom format
struct PrintableTime {
    hours: i64,
    minutes: i64,
    seconds: i64,
}
impl PrintableTime {
    fn from_duration(d: Duration) -> Self {
        let h = d.num_hours();
        let m = d.num_minutes() - (60 * h);
        let s = d.num_seconds() - (60 * m);
        return Self {
            hours: h,
            minutes: m,
            seconds: s,
        };
    }
}
use std::{cmp::Ordering, fmt};
impl fmt::Display for PrintableTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.hours > 0 {
            write!(f, "{}hr:", self.hours)?
        }
        if self.minutes > 0 {
            write!(f, "{}m:", self.minutes)?
        }
        write!(f, "{}s", self.seconds)
    }
}

//used to check if we're before or after a class
use chrono::{prelude::Local, Duration};
#[derive(PartialEq)]
enum RelPos {
    Before,
    In,
    After,
}

static CSVTIMES: Mutex<Vec<ClassTime>> = Mutex::new(Vec::new());
static DRAWN_GLOBAL: Mutex<LastDrawn> = Mutex::new(LastDrawn::None);
//used to check if we've already drawn the same thing
#[derive(PartialEq, Debug, Clone, Copy)]
enum LastDrawn {
    Time,   //time left(DrawType::In)
    Before, //DrawType::Before
    Out,    //DrawType::Out
    None,   //haven't drawn yet
}
pub enum DrawType {
    In,     //in a class
    Before, //right before a class
    Out,    //out of classes for the day(RelPos::After for all)
}
pub type DrawFn = fn(
    draw_type: DrawType,
    redraw_all: bool,
    class: &String,
    time_left: &String,
    current_time: &String,
);
static EMPTY: String = String::new();
pub fn get_time_left(draw: &DrawFn) {
    let local_time = Local::now();
    let current_time: NaiveTime = local_time.time();
    let mut time_class_pos: Vec<RelPos> = Vec::new(); //time position relative to each class
    let mut found_class = false;
    let csvtimes_local = CSVTIMES.lock().unwrap();
    let mut drawn = DRAWN_GLOBAL.lock().unwrap();
    let current_day = local_time.weekday();
    for i in csvtimes_local
        .iter()
        .filter(|x| x.days.contains(&current_day))
    {
        //make times with u8's and such

        if (i.begin..=i.end).contains(&current_time) {
            time_class_pos.push(RelPos::In);

            let d = i.end.signed_duration_since(current_time);
            let p = PrintableTime::from_duration(d);

            let time_left = format!("{}", p);
            let current_time = current_time.format("%I:%M:%S %P").to_string();
            // let (x, y) = (0,0);
            draw(
                DrawType::In,
                *drawn != LastDrawn::Time,
                &i.class,
                &time_left,
                &current_time,
            );

            *drawn = LastDrawn::Time;
            found_class = true;
        } else {
            time_class_pos.push(match i.begin.cmp(&current_time) {
                Ordering::Greater => RelPos::Before,
                Ordering::Equal => unreachable!(), //In
                Ordering::Less => match i.end.cmp(&current_time) {
                    Ordering::Greater | Ordering::Equal => unreachable!(), //In
                    Ordering::Less => RelPos::After,
                },
            });
        }
    } //loop thru each class
    if found_class == false {
        //none is RelPos::In
        //find first one(its index) that is RelPos::Before
        let mut first_before_i = None;
        for (i, pos) in time_class_pos.iter().enumerate() {
            if pos == &RelPos::Before {
                first_before_i = Some(i);
                break;
            }
        }
        //if the first class isn't before, then it's after
        if let Some(first_before_i) = first_before_i {
            if *drawn != LastDrawn::Before {
                let first_before = &csvtimes_local[first_before_i];
                let d = first_before.end.signed_duration_since(current_time);
                let p = PrintableTime::from_duration(d);

                let time_left = format!("{}", p);
                let current_time = current_time.format("%I:%M:%S %P").to_string();
                draw(
                    DrawType::Before,
                    *drawn != LastDrawn::Before,
                    &first_before.class,
                    &time_left,
                    &current_time,
                );
            }
            *drawn = LastDrawn::Before;
        } else if *drawn != LastDrawn::Out {
            //we know we're out bc all are RelPoss:After bc we have searched for the first RelPos::In and RelPos::Before and failed
            if *drawn != LastDrawn::Out {
                let current_time = current_time.format("%I:%M:%S %P").to_string();
                draw(DrawType::Out, true, &EMPTY, &EMPTY, &current_time);
            }
            *drawn = LastDrawn::Out;
        }
    }
}
pub fn set_csv(csv: Vec<ClassTime>) {
    *CSVTIMES.lock().unwrap() = csv;
}

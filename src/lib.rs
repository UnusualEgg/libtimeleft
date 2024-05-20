use chrono::{Datelike, NaiveTime, Weekday};
use serde::de::{IntoDeserializer, Visitor};

use serde::{Deserialize, Deserializer};
use std::fmt::Display;
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::Mutex;

#[derive(PartialEq, Debug, Clone, Copy)]
enum LastDrawn {
    Time,
    Between,
    Out,
    None,
}
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
#[derive(Debug, Deserialize)]
pub struct CSVTime {
    #[serde(deserialize_with = "space_separated")]
    days: Vec<Weekday>,
    name: String,
    begin: String,
    end: String,
}
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
fn add_zero(n: i64) -> String {
    if n < 10 {
        let mut s = n.to_string();
        s.insert(0, '0');
        return s;
    } else {
        return n.to_string();
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

use chrono::{prelude::Local, Duration};
#[derive(PartialEq)]
enum RelPos {
    Before,
    In,
    After,
}

static CSVTIMES: Mutex<Vec<ClassTime>> = Mutex::new(Vec::new());
static DRAWN_GLOBAL: Mutex<LastDrawn> = Mutex::new(LastDrawn::None);
pub enum DrawType {
    In,
    Before,
    Out,
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
                Ordering::Equal => RelPos::In,
                Ordering::Less => match i.end.cmp(&current_time) {
                    Ordering::Greater | Ordering::Equal => RelPos::In,
                    Ordering::Less => RelPos::After,
                },
            });
        }
    } //loop thru each class
    if found_class == false {
        //check if we before first class or after lass. then: we in between. else: we out of school
        let first = time_class_pos.first();
        let last = time_class_pos.last();
        if ((first == Some(&RelPos::After)) || (first == Some(&RelPos::In)))
            && ((last == Some(&RelPos::In)) || (last == Some(&RelPos::Before)))
        {
            if *drawn != LastDrawn::Between {
                //TODO find which class is next
                'classes: for i in 0..time_class_pos.len() {
                    match time_class_pos[i] {
                        RelPos::After => (),
                        RelPos::Before => {
                            let d = csvtimes_local[i].end.signed_duration_since(current_time);
                            let p = PrintableTime::from_duration(d);

                            let time_left = format!("{}", p);
                            let current_time = current_time.format("%I:%M:%S %P").to_string();
                            draw(
                                DrawType::Before,
                                *drawn != LastDrawn::Between,
                                &csvtimes_local[i].class,
                                &time_left,
                                &current_time,
                            );
                            break 'classes;
                        }
                        RelPos::In => (),
                    }
                }
            }
            *drawn = LastDrawn::Between;
        } else if *drawn != LastDrawn::Out {
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

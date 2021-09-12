use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::ops::Range;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, Copy, Clone, PartialEq)]
enum Speed {
    Kmph(f64),
    Ms(f64),
    SecPer500m(f64),
}

impl PartialOrd for Speed {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.to_ms().partial_cmp(&other.to_ms())
    }
}

impl Speed {
    /// Convert the speed to a meters per second
    fn to_ms(self) -> f64 {
        match self {
            Speed::Kmph(kmph) => kmph / 3.6,
            Speed::Ms(ms) => ms,
            Speed::SecPer500m(pace) => 500.0 / pace,
        }
    }

    // /// Convert the speed to kilometers per hour
    // fn to_kmph(self) -> f64 {
    //     match self {
    //         Speed::Kmph(kmph) => kmph,
    //         Speed::Ms(ms) => ms * 3.6,
    //         Speed::SecPer500m(pace) => 500.0 / pace * 3.6,
    //     }
    // }
    //
    // /// Convert the speed to pace
    // fn to_pace(self) -> f64 {
    //     match self {
    //         Speed::Kmph(kmph) => 500.0 * 3.6 / kmph,
    //         Speed::Ms(ms) => 500.0 / ms,
    //         Speed::SecPer500m(pace) => pace,
    //     }
    // }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "interval_detector", about = "Find intervals from CSV files")]
struct Opt {
    /// The average speed in Km/hour of an interval
    #[structopt(long, short = "k")]
    limit_kmph: Option<f64>,

    /// The average speed in seconds per 500m of an interval
    #[structopt(long, short = "p")]
    limit_pace: Option<f64>,

    /// The minimum duration of an interval
    #[structopt(short, long, default_value = "20")]
    min_interval_duration: usize,

    /// Input file
    #[structopt(parse(from_os_str))]
    input: PathBuf,
}

#[derive(Debug, Deserialize)]
struct RawRecord {
    #[serde(rename(deserialize = "time"))]
    time_in_seconds: usize,

    #[serde(rename(deserialize = "activityType"))]
    activity_type: isize,

    #[serde(rename(deserialize = "lapNumber"))]
    lap_number: Option<usize>,

    distance: Option<f64>,

    speed: Option<f64>,

    calories: Option<usize>,

    #[serde(rename(deserialize = "lat"))]
    latitide: Option<f64>,

    #[serde(rename(deserialize = "long"))]
    longtitude: Option<f64>,

    elevation: Option<f64>,

    #[serde(rename(deserialize = "heartRate"))]
    heart_rate: Option<String>,
    cycles: Option<usize>,
}

#[derive(Debug)]
struct Record {
    time_in_seconds: usize,
    distance: f64,
    speed: Speed,
}

fn find_interval(records: &[Record], start_index: usize, limit: Speed) -> Option<Range<usize>> {
    let start_index = records
        .iter()
        .enumerate()
        .skip(start_index)
        .find_map(|(idx, rec)| if rec.speed >= limit { Some(idx) } else { None })?;

    let mut total_speed = 0.0;
    for (idx, rec) in records.iter().enumerate().skip(start_index) {
        total_speed += rec.speed.to_ms();
        let average_speed_ms = total_speed / (idx - start_index + 1) as f64;
        if Speed::Ms(average_speed_ms) < limit {
            return Some(start_index..idx);
        }
    }

    None
}

fn find_all_intervals(records: &[Record], limit: Speed) -> Vec<Range<usize>> {
    let mut results = Vec::new();
    let mut start_index = 0;
    loop {
        match find_interval(records, start_index, limit) {
            Some(interval) => {
                start_index = interval.end;
                results.push(interval);
            }
            None => {
                return results;
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct IntervalInfo {
    start_time: usize,
    duration: usize,
    distance: usize,
}

fn main() {
    let args = Opt::from_args();

    let limit = if !(args.limit_kmph.is_some() ^ args.limit_pace.is_some()) {
        println!("error: must specify either --limit-kmph or --limit-pace");
        return;
    } else if let Some(limit) = args.limit_kmph {
        Speed::Kmph(limit)
    } else if let Some(limit) = args.limit_pace {
        Speed::SecPer500m(limit)
    } else {
        unreachable!()
    };

    // Iterate over all records
    let mut records: Vec<RawRecord> = csv::Reader::from_path(&args.input)
        .expect("could not open input file")
        .into_deserialize()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    // If the last field has activityType=-1, remove it
    if matches!(
        records.last(),
        Some(RawRecord {
            activity_type: -1,
            ..
        })
    ) {
        records.pop();
    }

    // TODO: Find gaps in the timeline

    // Convert to something we can work with
    let records = records
        .into_iter()
        .map(|raw| Record {
            time_in_seconds: raw.time_in_seconds,
            distance: raw.distance.unwrap(),
            speed: Speed::Ms(raw.speed.unwrap()),
        })
        .collect::<Vec<_>>();

    let intervals = find_all_intervals(&records, limit)
        .into_iter()
        .filter(|range| {
            records[range.end - 1].time_in_seconds - records[range.start].time_in_seconds
                >= args.min_interval_duration
        })
        .map(|range| IntervalInfo {
            start_time: records[range.start].time_in_seconds,
            duration: records[range.end - 1].time_in_seconds - records[range.start].time_in_seconds,
            distance: (records[range.end - 1].distance - records[range.start].distance).round()
                as usize,
        })
        .collect::<Vec<_>>();

    let mut wrtr = csv::Writer::from_writer(std::io::stdout());
    for interval in intervals {
        wrtr.serialize(interval).unwrap();
    }
    wrtr.flush().unwrap();
}

#[cfg(test)]
mod test {
    use crate::{find_interval, Record, Speed};

    #[test]
    fn test_find_interval() {
        let records = [
            Record {
                time_in_seconds: 0,
                speed: Speed::Ms(1.0),
                distance: 0.0,
            },
            Record {
                time_in_seconds: 1,
                speed: Speed::Ms(2.0),
                distance: 1.0,
            },
            Record {
                time_in_seconds: 2,
                speed: Speed::Ms(1.8),
                distance: 2.0,
            },
            Record {
                time_in_seconds: 3,
                speed: Speed::Ms(2.2),
                distance: 2.0,
            },
            Record {
                time_in_seconds: 4,
                speed: Speed::Ms(0.0),
                distance: 2.0,
            },
        ];

        assert_eq!(find_interval(&records, 0, Speed::Ms(1.9)), Some(1..4));
        assert_eq!(find_interval(&records, 0, Speed::Ms(2.1)), Some(3..4));
    }
}

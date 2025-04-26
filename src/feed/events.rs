//! Feed event types for DXLink feed service
//!
//! This module contains the event type definitions for the feed service,
//! including all event subtypes like QuoteEvent, TradeEvent, etc.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Custom type to handle JSON double values including special values like NaN, Infinity, etc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum JSONDouble {
    /// Regular numeric value
    Number(f64),
    /// Not a number - "NaN"
    #[serde(deserialize_with = "deserialize_nan", serialize_with = "serialize_nan")]
    NaN,
    /// Positive infinity - "Infinity"
    #[serde(
        deserialize_with = "deserialize_infinity",
        serialize_with = "serialize_infinity"
    )]
    Infinity,
    /// Negative infinity - "-Infinity"
    #[serde(
        deserialize_with = "deserialize_neg_infinity",
        serialize_with = "serialize_neg_infinity"
    )]
    NegInfinity,
}

impl JSONDouble {
    /// Converts JSONDouble to f64
    pub fn to_f64(&self) -> f64 {
        match self {
            JSONDouble::Number(n) => *n,
            JSONDouble::NaN => f64::NAN,
            JSONDouble::Infinity => f64::INFINITY,
            JSONDouble::NegInfinity => f64::NEG_INFINITY,
        }
    }

    /// Checks if this value is NaN
    pub fn is_nan(&self) -> bool {
        matches!(self, JSONDouble::NaN)
    }

    /// Checks if this value is infinite (positive or negative)
    pub fn is_infinite(&self) -> bool {
        matches!(self, JSONDouble::Infinity | JSONDouble::NegInfinity)
    }

    /// Checks if this value is finite (normal number)
    pub fn is_finite(&self) -> bool {
        matches!(self, JSONDouble::Number(_))
    }
}

impl From<f64> for JSONDouble {
    fn from(value: f64) -> Self {
        if value.is_nan() {
            JSONDouble::NaN
        } else if value.is_infinite() {
            if value.is_sign_positive() {
                JSONDouble::Infinity
            } else {
                JSONDouble::NegInfinity
            }
        } else {
            JSONDouble::Number(value)
        }
    }
}

impl From<JSONDouble> for f64 {
    fn from(value: JSONDouble) -> Self {
        value.to_f64()
    }
}

impl fmt::Display for JSONDouble {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JSONDouble::Number(n) => write!(f, "{}", n),
            JSONDouble::NaN => write!(f, "NaN"),
            JSONDouble::Infinity => write!(f, "Infinity"),
            JSONDouble::NegInfinity => write!(f, "-Infinity"),
        }
    }
}

// Custom deserialization functions (as before)
fn deserialize_nan<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    if s == "NaN" {
        Ok(())
    } else {
        Err(serde::de::Error::custom("Expected \"NaN\""))
    }
}

fn deserialize_infinity<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    if s == "Infinity" {
        Ok(())
    } else {
        Err(serde::de::Error::custom("Expected \"Infinity\""))
    }
}

fn deserialize_neg_infinity<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    if s == "-Infinity" {
        Ok(())
    } else {
        Err(serde::de::Error::custom("Expected \"-Infinity\""))
    }
}

// Custom serialization functions
fn serialize_nan<S>(serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str("NaN")
}

fn serialize_infinity<S>(serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str("Infinity")
}

fn serialize_neg_infinity<S>(serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str("-Infinity")
}

/// Union type for all feed event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "eventType")]
pub enum FeedEvent {
    /// Quote event with bid/ask prices, sizes, etc.
    Quote(QuoteEvent),
    /// Profile event with security instrument details
    Profile(ProfileEvent),
    /// Trade event with last trade information
    Trade(TradeEvent),
    /// Extended trading hours trade event
    TradeETH(TradeETHEvent),
    /// Candle event with OHLC data
    Candle(CandleEvent),
    /// Summary event with session information
    Summary(SummaryEvent),
    /// Time and sale event for trade history
    TimeAndSale(TimeAndSaleEvent),
    /// Greeks event with option pricing data
    Greeks(GreeksEvent),
    /// Theoretical price event
    TheoPrice(TheoPriceEvent),
    /// Underlying event with option underlying data
    Underlying(UnderlyingEvent),
    /// Option sale event
    OptionSale(OptionSaleEvent),
    /// Series event with option series data
    Series(SeriesEvent),
    /// Order event with market depth information
    Order(OrderEvent),
    /// Spread order event
    SpreadOrder(SpreadOrderEvent),
    /// Analytic order event with enhanced information
    AnalyticOrder(AnalyticOrderEvent),
    /// Configuration event
    Configuration(ConfigurationEvent),
    /// Message event (text messages and notifications)
    Message(MessageEvent),
}

/// Quote event with bid/ask information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuoteEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_time: Option<u64>,
    /// Sequence number for ordering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
    /// Nanosecond part of the timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_nano_part: Option<u64>,
    /// Bid timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bid_time: Option<u64>,
    /// Exchange code for the bid
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bid_exchange_code: Option<String>,
    /// Bid price
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bid_price: Option<JSONDouble>,
    /// Bid size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bid_size: Option<JSONDouble>,
    /// Ask timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask_time: Option<u64>,
    /// Exchange code for the ask
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask_exchange_code: Option<String>,
    /// Ask price
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask_price: Option<JSONDouble>,
    /// Ask size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask_size: Option<JSONDouble>,
}

/// Profile event with security instrument details
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Product description
    pub description: Option<String>,
    /// Short sale restriction status
    pub short_sale_restriction: Option<String>,
    /// Trading status
    pub trading_status: Option<String>,
    /// Halt start timestamp
    pub halt_start_time: Option<u64>,
    /// Halt end timestamp
    pub halt_end_time: Option<u64>,
    /// High limit price
    pub high_limit_price: Option<JSONDouble>,
    /// Low limit price
    pub low_limit_price: Option<JSONDouble>,
    /// 52-week high price
    pub high52_week_price: Option<JSONDouble>,
    /// 52-week low price
    pub low52_week_price: Option<JSONDouble>,
    /// Beta value
    pub beta: Option<JSONDouble>,
    /// Earnings per share
    pub earnings_per_share: Option<JSONDouble>,
    /// Dividend frequency
    pub dividend_frequency: Option<JSONDouble>,
    /// Ex-dividend amount
    pub ex_dividend_amount: Option<JSONDouble>,
    /// Ex-dividend day ID
    pub ex_dividend_day_id: Option<u64>,
    /// Shares outstanding
    pub shares: Option<JSONDouble>,
    /// Free float percentage
    pub free_float: Option<JSONDouble>,
}

/// Trade event with last trade information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TradeEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Sequence number for ordering
    pub sequence: Option<u64>,
    /// Nanosecond part of the timestamp
    pub time_nano_part: Option<u64>,
    /// Trade timestamp
    pub time: Option<u64>,
    /// Exchange code where the trade occurred
    pub exchange_code: Option<String>,
    /// Trade price
    pub price: Option<JSONDouble>,
    /// Change from previous close
    pub change: Option<JSONDouble>,
    /// Trade size
    pub size: Option<JSONDouble>,
    /// Day ID (trading session identifier)
    pub day_id: Option<u64>,
    /// Total volume for the day
    pub day_volume: Option<JSONDouble>,
    /// Total turnover (volume*price) for the day
    pub day_turnover: Option<JSONDouble>,
    /// Direction of the tick (up, down, zero)
    pub tick_direction: Option<String>,
    /// Whether this trade occurred in extended trading hours
    pub extended_trading_hours: Option<bool>,
}

/// Extended trading hours trade event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TradeETHEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Sequence number for ordering
    pub sequence: Option<u64>,
    /// Nanosecond part of the timestamp
    pub time_nano_part: Option<u64>,
    /// Trade timestamp
    pub time: Option<u64>,
    /// Exchange code where the trade occurred
    pub exchange_code: Option<String>,
    /// Trade price
    pub price: Option<JSONDouble>,
    /// Change from previous close
    pub change: Option<JSONDouble>,
    /// Trade size
    pub size: Option<JSONDouble>,
    /// Day ID (trading session identifier)
    pub day_id: Option<u64>,
    /// Total volume for the day
    pub day_volume: Option<JSONDouble>,
    /// Total turnover (volume*price) for the day
    pub day_turnover: Option<JSONDouble>,
    /// Direction of the tick (up, down, zero)
    pub tick_direction: Option<String>,
    /// Whether this trade occurred in extended trading hours
    pub extended_trading_hours: Option<bool>,
}

/// Candle event with OHLC data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CandleEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Day ID (trading session identifier)
    pub day_id: Option<u64>,
    /// Timestamp for the start of this candle
    pub time: Option<u64>,
    /// Sequence number for ordering
    pub sequence: Option<u64>,
    /// Open price
    pub open: Option<JSONDouble>,
    /// High price
    pub high: Option<JSONDouble>,
    /// Low price
    pub low: Option<JSONDouble>,
    /// Close price
    pub close: Option<JSONDouble>,
    /// Volume traded
    pub volume: Option<JSONDouble>,
    /// Volume-weighted average price
    pub vwap: Option<JSONDouble>,
    /// Bid volume
    pub bid_volume: Option<JSONDouble>,
    /// Ask volume
    pub ask_volume: Option<JSONDouble>,
    /// Implied volatility
    pub imp_volatility: Option<JSONDouble>,
    /// Open interest
    pub open_interest: Option<JSONDouble>,
}

/// Summary event with session information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Day ID (trading session identifier)
    pub day_id: Option<u64>,
    /// Day open price
    pub day_open_price: Option<JSONDouble>,
    /// Day high price
    pub day_high_price: Option<JSONDouble>,
    /// Day low price
    pub day_low_price: Option<JSONDouble>,
    /// Day close price
    pub day_close_price: Option<JSONDouble>,
    /// Day close price type
    pub day_close_price_type: Option<String>,
    /// Previous day ID
    pub prev_day_id: Option<u64>,
    /// Previous day close price
    pub prev_day_close_price: Option<JSONDouble>,
    /// Previous day close price type
    pub prev_day_close_price_type: Option<String>,
    /// Previous day volume
    pub prev_day_volume: Option<JSONDouble>,
    /// Open interest
    pub open_interest: Option<u64>,
}

/// Time and sale event for trade history
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimeAndSaleEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Time and sale timestamp
    pub time: Option<u64>,
    /// Sequence number for ordering
    pub sequence: Option<u64>,
    /// Exchange code where the trade occurred
    pub exchange_code: Option<String>,
    /// Trade price
    pub price: Option<JSONDouble>,
    /// Trade size
    pub size: Option<JSONDouble>,
    /// Bid price at the time of the trade
    pub bid_price: Option<JSONDouble>,
    /// Ask price at the time of the trade
    pub ask_price: Option<JSONDouble>,
    /// Exchange sale conditions
    pub exchange_sale_conditions: Option<String>,
    /// Trade through exempt flag
    pub trade_through_exempt: Option<String>,
    /// Side that initiated the trade (buy, sell)
    pub aggressor_side: Option<String>,
    /// Whether this was a spread leg trade
    pub spread_leg: Option<bool>,
    /// Whether this trade occurred in extended trading hours
    pub extended_trading_hours: Option<bool>,
}

/// Greeks event with option pricing data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GreeksEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Price
    pub price: Option<JSONDouble>,
    /// Volatility
    pub volatility: Option<JSONDouble>,
    /// Delta - rate of change of option price relative to underlying price
    pub delta: Option<JSONDouble>,
    /// Gamma - rate of change of delta relative to underlying price
    pub gamma: Option<JSONDouble>,
    /// Theta - rate of change of option price relative to time
    pub theta: Option<JSONDouble>,
    /// Rho - rate of change of option price relative to interest rate
    pub rho: Option<JSONDouble>,
    /// Vega - rate of change of option price relative to volatility
    pub vega: Option<JSONDouble>,
}

/// Theoretical price event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TheoPriceEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Event flags
    pub event_flags: Option<u64>,
    /// Theoretical price
    pub price: Option<JSONDouble>,
    /// Underlying price
    pub underlying_price: Option<JSONDouble>,
    /// Delta
    pub delta: Option<JSONDouble>,
    /// Gamma
    pub gamma: Option<JSONDouble>,
    /// Dividend
    pub dividend: Option<JSONDouble>,
    /// Interest rate
    pub interest: Option<JSONDouble>,
}

/// Underlying event with option underlying data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnderlyingEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Volatility
    pub volatility: Option<JSONDouble>,
    /// Front month volatility
    pub front_volatility: Option<JSONDouble>,
    /// Back month volatility
    pub back_volatility: Option<JSONDouble>,
    /// Call option volume
    pub call_volume: Option<JSONDouble>,
    /// Put option volume
    pub put_volume: Option<JSONDouble>,
    /// Put/Call ratio
    pub put_call_ratio: Option<JSONDouble>,
}

/// Option sale event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OptionSaleEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Sale time
    pub time: Option<u64>,
    /// Exchange code where the sale occurred
    pub exchange_code: Option<String>,
    /// Sale price
    pub price: Option<JSONDouble>,
    /// Sale size
    pub size: Option<JSONDouble>,
    /// Sale conditions
    pub sale_conditions: Option<String>,
    /// Side that initiated the sale (buy, sell)
    pub side: Option<String>,
    /// Implied volatility
    pub volatility: Option<JSONDouble>,
    /// Delta
    pub delta: Option<JSONDouble>,
    /// Option symbol
    pub option_symbol: Option<String>,
}

/// Series event with option series data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SeriesEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Series
    pub series: Option<String>,
    /// Expiration date
    pub expiration: Option<u64>,
    /// Volatility
    pub volatility: Option<JSONDouble>,
    /// Call volume
    pub call_volume: Option<JSONDouble>,
    /// Put volume
    pub put_volume: Option<JSONDouble>,
    /// Put/Call ratio
    pub put_call_ratio: Option<JSONDouble>,
    /// Forward price
    pub forward_price: Option<JSONDouble>,
    /// Dividend
    pub dividend: Option<JSONDouble>,
    /// Interest rate
    pub interest: Option<JSONDouble>,
}

/// Order event with market depth information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrderEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Event flags
    pub event_flags: Option<u64>,
    /// Order index/ID
    pub index: Option<u64>,
    /// Order timestamp
    pub time: Option<u64>,
    /// Order sequence number
    pub sequence: Option<u64>,
    /// Order price
    pub price: Option<JSONDouble>,
    /// Order size
    pub size: Option<JSONDouble>,
    /// Executed size so far
    pub executed_size: Option<JSONDouble>,
    /// Number of orders at this price level
    pub count: Option<u64>,
    /// Exchange code
    pub exchange_code: Option<String>,
    /// Order side (buy, sell)
    pub order_side: Option<String>,
    /// Order scope
    pub scope: Option<String>,
    /// Trade ID if the order resulted in a trade
    pub trade_id: Option<u64>,
    /// Trade price if the order resulted in a trade
    pub trade_price: Option<JSONDouble>,
    /// Trade size if the order resulted in a trade
    pub trade_size: Option<JSONDouble>,
}

/// Spread order event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpreadOrderEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Event flags
    pub event_flags: Option<u64>,
    /// Order index/ID
    pub index: Option<u64>,
    /// Order timestamp
    pub time: Option<u64>,
    /// Order sequence number
    pub sequence: Option<u64>,
    /// Order price
    pub price: Option<JSONDouble>,
    /// Order size
    pub size: Option<JSONDouble>,
    /// Executed size so far
    pub executed_size: Option<JSONDouble>,
    /// Number of orders at this price level
    pub count: Option<u64>,
    /// Exchange code
    pub exchange_code: Option<String>,
    /// Order side (buy, sell)
    pub order_side: Option<String>,
    /// Order scope
    pub scope: Option<String>,
    /// Trade ID if the order resulted in a trade
    pub trade_id: Option<u64>,
    /// Trade price if the order resulted in a trade
    pub trade_price: Option<JSONDouble>,
    /// Trade size if the order resulted in a trade
    pub trade_size: Option<JSONDouble>,
}

/// Analytic order event with enhanced information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticOrderEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Event flags
    pub event_flags: Option<u64>,
    /// Order index/ID
    pub index: Option<u64>,
    /// Order timestamp
    pub time: Option<u64>,
    /// Iceberg peak size for iceberg orders
    pub iceberg_peak_size: Option<JSONDouble>,
    /// Iceberg hidden size
    pub iceberg_hidden_size: Option<JSONDouble>,
    /// Iceberg executed size
    pub iceberg_executed_size: Option<JSONDouble>,
    /// Iceberg order type
    pub iceberg_type: Option<String>,
    /// Market maker ID
    pub market_maker: Option<String>,
    /// Order sequence number
    pub sequence: Option<u64>,
    /// Order price
    pub price: Option<JSONDouble>,
    /// Order size
    pub size: Option<JSONDouble>,
    /// Executed size so far
    pub executed_size: Option<JSONDouble>,
    /// Number of orders at this price level
    pub count: Option<u64>,
    /// Exchange code
    pub exchange_code: Option<String>,
    /// Order side (buy, sell)
    pub order_side: Option<String>,
    /// Order scope
    pub scope: Option<String>,
    /// Trade ID if the order resulted in a trade
    pub trade_id: Option<u64>,
    /// Trade price if the order resulted in a trade
    pub trade_price: Option<JSONDouble>,
    /// Trade size if the order resulted in a trade
    pub trade_size: Option<JSONDouble>,
}

/// Configuration event with service configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationEvent {
    /// Symbol for this event
    pub event_symbol: String,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Object containing configuration values
    pub configuration: Option<serde_json::Value>,
}

/// Message event for text notifications
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessageEvent {
    /// Symbol for this event (if applicable)
    pub event_symbol: Option<String>,
    /// Timestamp of this event
    pub event_time: Option<u64>,
    /// Message text
    pub message: String,
    /// Message type/severity (info, warning, error)
    pub message_type: Option<String>,
    /// Message code for identifying specific messages
    pub message_code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_jsondouble_serialization() {
        // Regular number
        let num = JSONDouble::Number(42.5);
        let serialized = serde_json::to_string(&num).unwrap();
        assert_eq!(serialized, "42.5");

        // Special values
        let nan = JSONDouble::NaN;
        let serialized = serde_json::to_string(&nan).unwrap();
        assert_eq!(serialized, "\"NaN\"");

        let inf = JSONDouble::Infinity;
        let serialized = serde_json::to_string(&inf).unwrap();
        assert_eq!(serialized, "\"Infinity\"");

        let neg_inf = JSONDouble::NegInfinity;
        let serialized = serde_json::to_string(&neg_inf).unwrap();
        assert_eq!(serialized, "\"-Infinity\"");
    }

    #[test]
    fn test_jsondouble_deserialization() {
        // Regular number
        let deserialized: JSONDouble = serde_json::from_str("42.5").unwrap();
        assert!(matches!(deserialized, JSONDouble::Number(n) if n == 42.5));

        // Special values as strings
        let deserialized: JSONDouble = serde_json::from_str("\"NaN\"").unwrap();
        assert!(matches!(deserialized, JSONDouble::NaN));

        let deserialized: JSONDouble = serde_json::from_str("\"Infinity\"").unwrap();
        assert!(matches!(deserialized, JSONDouble::Infinity));

        let deserialized: JSONDouble = serde_json::from_str("\"-Infinity\"").unwrap();
        assert!(matches!(deserialized, JSONDouble::NegInfinity));
    }

    #[test]
    fn test_jsondouble_conversion() {
        // From f64 to JSONDouble
        assert!(matches!(JSONDouble::from(42.5), JSONDouble::Number(n) if n == 42.5));
        assert!(matches!(JSONDouble::from(f64::NAN), JSONDouble::NaN));
        assert!(matches!(
            JSONDouble::from(f64::INFINITY),
            JSONDouble::Infinity
        ));
        assert!(matches!(
            JSONDouble::from(f64::NEG_INFINITY),
            JSONDouble::NegInfinity
        ));

        // From JSONDouble to f64
        let f: f64 = JSONDouble::Number(42.5).into();
        assert_eq!(f, 42.5);

        let f: f64 = JSONDouble::NaN.into();
        assert!(f.is_nan());

        let f: f64 = JSONDouble::Infinity.into();
        assert!(f.is_infinite() && f.is_sign_positive());

        let f: f64 = JSONDouble::NegInfinity.into();
        assert!(f.is_infinite() && f.is_sign_negative());
    }

    #[test]
    fn test_quote_event_deserialization() {
        let json_data = json!({
            "eventType": "Quote",
            "eventSymbol": "AAPL",
            "eventTime": 1623456789000 as i64,
            "bidPrice": 145.75,
            "askPrice": 145.78,
            "bidSize": 100,
            "askSize": 200
        });

        let event: FeedEvent = serde_json::from_value(json_data).unwrap();
        match event {
            FeedEvent::Quote(quote) => {
                assert_eq!(quote.event_symbol, "AAPL");
                assert_eq!(quote.event_time, Some(1623456789000));
                assert!(matches!(quote.bid_price, Some(JSONDouble::Number(n)) if n == 145.75));
                assert!(matches!(quote.ask_price, Some(JSONDouble::Number(n)) if n == 145.78));
                assert!(matches!(quote.bid_size, Some(JSONDouble::Number(n)) if n == 100.0));
                assert!(matches!(quote.ask_size, Some(JSONDouble::Number(n)) if n == 200.0));
            }
            _ => panic!("Expected Quote event"),
        }
    }

    #[test]
    fn test_special_jsondouble_in_event() {
        let json_data = json!({
            "eventType": "Quote",
            "eventSymbol": "AAPL",
            "eventTime": 1623456789000 as i64,
            "bidPrice": "NaN",
            "askPrice": "Infinity",
            "bidSize": "-Infinity",
            "askSize": 200
        });

        let event: FeedEvent = serde_json::from_value(json_data).unwrap();
        match event {
            FeedEvent::Quote(quote) => {
                assert_eq!(quote.event_symbol, "AAPL");
                assert!(matches!(quote.bid_price, Some(JSONDouble::NaN)));
                assert!(matches!(quote.ask_price, Some(JSONDouble::Infinity)));
                assert!(matches!(quote.bid_size, Some(JSONDouble::NegInfinity)));
                assert!(matches!(quote.ask_size, Some(JSONDouble::Number(n)) if n == 200.0));
            }
            _ => panic!("Expected Quote event"),
        }
    }

    #[test]
    fn test_trade_event_deserialization() {
        let json_data = json!({
            "eventType": "Trade",
            "eventSymbol": "MSFT",
            "eventTime": 1623456789000 as i64,
            "price": 290.45,
            "size": 50,
            "dayVolume": 1000000
        });

        let event: FeedEvent = serde_json::from_value(json_data).unwrap();
        match event {
            FeedEvent::Trade(trade) => {
                assert_eq!(trade.event_symbol, "MSFT");
                assert_eq!(trade.event_time, Some(1623456789000));
                assert!(matches!(trade.price, Some(JSONDouble::Number(n)) if n == 290.45));
                assert!(matches!(trade.size, Some(JSONDouble::Number(n)) if n == 50.0));
                assert!(matches!(trade.day_volume, Some(JSONDouble::Number(n)) if n == 1000000.0));
            }
            _ => panic!("Expected Trade event"),
        }
    }
}

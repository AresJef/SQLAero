//! Native contract tests independent of any target format.

use num_bigint::BigInt;
use sqlaero_transcode::value::{Consumer, ScalarRef};

#[test]
fn native_facts_reach_a_non_sql_consumer_without_numeric_policy() {
    struct Inspect;
    impl Consumer for Inspect {
        type Context = ();
        type Error = &'static str;

        fn write(&mut self, value: ScalarRef<'_>, _: &()) -> Result<(), Self::Error> {
            match value {
                ScalarRef::BigInteger(value) => {
                    assert_eq!(value.bits(), 333);
                }
                ScalarRef::Float(value) => assert_eq!(value.to_bits(), (-0.0_f64).to_bits()),
                _ => return Err("unexpected scalar"),
            }
            Ok(())
        }
        fn text_start(&mut self, _: usize, _: &()) -> Result<(), Self::Error> {
            Ok(())
        }
        fn text_fragment(&mut self, _: &str, _: &()) -> Result<(), Self::Error> {
            Ok(())
        }
        fn text_end(&mut self, _: &()) -> Result<(), Self::Error> {
            Ok(())
        }
        fn bytes_start(&mut self, _: usize, _: &()) -> Result<(), Self::Error> {
            Ok(())
        }
        fn bytes_fragment(&mut self, _: &[u8], _: &()) -> Result<(), Self::Error> {
            Ok(())
        }
        fn bytes_end(&mut self, _: &()) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    let integer = BigInt::from(10_u8).pow(100);
    Inspect.write(ScalarRef::BigInteger(&integer), &()).unwrap();
    Inspect.write(ScalarRef::Float(-0.0), &()).unwrap();
}

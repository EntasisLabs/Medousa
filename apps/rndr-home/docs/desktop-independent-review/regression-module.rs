
#[cfg(test)]
mod independent_review_tests {
use super::*;
fn settle(a: &mut App) { for _ in 0..8 { a.update(); } }
#[test]
fn focus_on_preferences_control_survives_responsive_rebuild() {
 let mut a=app(); a.update();
 desktop::navigate(a.world_mut(),admin_model::Route::Settings(0)); settle(&mut a);
 let e=a.world().entities_with::<Button>().into_iter().find(|e|a.world().get::<Button>(*e).unwrap().action=="appearance:light").unwrap();
 request_focus(a.world_mut(),e); settle(&mut a);
 assert_eq!(a.world().resource::<Focus>().unwrap().entity,Some(e));
 a.world_mut().insert_resource(Viewport::new(480.,760.)); settle(&mut a);
 let f=a.world().resource::<Focus>().unwrap().entity;
 assert!(f.is_some_and(|e|a.world().has::<Focusable>(e)),"focus lost/deleted on responsive rebuild: {f:?}");
}
#[test]
fn unrelated_reply_preserves_current_dataset_scroll() {
 use admin_model::{Desktop,ReadState,Record,Resource,Route};
 let mut a=app(); a.update();
 a.world_mut().resource_mut::<Desktop>().unwrap().reads.insert(Resource::Runtimes,ReadState::Ready((0..80).map(|i|Record::new(format!("Row {i}"),vec![("Status","Snapshot".into())])).collect()));
 let serial=a.world_mut().resource_mut::<Desktop>().unwrap().begin(Resource::Health).unwrap();
 desktop::navigate(a.world_mut(),Route::Data(Resource::Runtimes)); settle(&mut a);
 let body=a.world().resource::<desktop::Handles>().unwrap().body;
 a.world_mut().get_mut::<Scroll>(body).unwrap().offset[1]=-200.; settle(&mut a);
 let before=a.world().get::<Scroll>(body).unwrap().offset; assert!(before[1]<0.);
 send(a.world_mut(),admin_backend::ReadReply{resource:Resource::Health,serial,result:Ok(vec![])}); settle(&mut a);
 assert_eq!(a.world().get::<Scroll>(body).unwrap().offset,before,"background Health reply reset Runtimes scroll");
}
}

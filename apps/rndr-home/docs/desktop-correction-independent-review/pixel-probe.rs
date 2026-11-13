#[cfg(test)]
mod pixel_probe {
 use super::*;
 #[test]
 fn export_long_list_detail_and_below_fold_settings() {
  use admin_model::{Desktop,ReadState,Record,Resource,Route};
  let dir=std::path::PathBuf::from(std::env::var("REVIEW_PIXEL_OUT").expect("explicit synthetic output")); std::fs::create_dir_all(&dir).unwrap();
  let mut a=app(); assert!(a.world().resource::<Model>().unwrap().commands.is_none());assert!(a.world().resource::<desktop::Connection>().unwrap().sender.is_none());
  a.world_mut().insert_resource(desktop_fixtures::SyntheticDesktop);
  a.world_mut().insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
  a.update();let mut exporter=rndr_render::Exporter::new().unwrap();
  for (name,width,appearance,contrast) in [("dark-narrow",480.,Appearance::Dark,Contrast::Standard),("light-wide",1120.,Appearance::Light,Contrast::Standard),("contrast-narrow",480.,Appearance::Dark,Contrast::High)] {
   a.world_mut().insert_resource(Viewport::new(width,650.));presentation::apply_theme(a.world_mut(),UiTheme{appearance,contrast,..Default::default()});
   for case in ["long-list","long-detail","settings-below-fold"] {
    a.world_mut().resource_mut::<Desktop>().unwrap().reads.insert(Resource::Runtimes,ReadState::Ready((0..80).map(|i| Record::new(format!("Synthetic row {i:03}"),(0..30).map(|_|("Field","Bounded synthetic long-detail value; no network.".into())).collect())).collect()));
    desktop::navigate(a.world_mut(),if case=="settings-below-fold" {Route::Settings(8)}else{Route::Data(Resource::Runtimes)});
    for _ in 0..8 {a.update();}
    let h=a.world().resource::<desktop::Handles>().unwrap().clone();
    if case=="long-detail" {desktop::action(a.world_mut(),"record:35");for _ in 0..8 {a.update();}}
    if case=="settings-below-fold" {
     a.world_mut().get_mut::<Scroll>(h.rail).unwrap().offset[1]=-10000.;
     if let Some(s)=a.world_mut().get_mut::<Scroll>(h.section_nav){s.offset[1]=-10000.;}
    } else {a.world_mut().get_mut::<Scroll>(h.body).unwrap().offset[1]=-700.;}
    for _ in 0..8 {a.update();}
    if case!="settings-below-fold" {assert!(a.world().get::<Scroll>(h.body).unwrap().offset[1]<0.);}
    let frame=rndr_paint::frame(&mut a);let img=exporter.render_frame(Some(a.world()),&frame,width as u32,650,1.).unwrap();
    std::fs::write(dir.join(format!("{name}-{case}.png")),rndr_render::encode_png(&img).unwrap()).unwrap();
   }
  }
 }
}
